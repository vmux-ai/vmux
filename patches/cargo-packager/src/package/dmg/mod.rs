// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// Copyright 2023-2023 CrabNebula Ltd.
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT
//
// Patched: replaced create-dmg script with direct hdiutil approach to work
// around macOS 26 Tahoe `hdiutil convert` mandatory file locking regression.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use super::Context;
use crate::{codesign::macos as codesign, shell::CommandExt, Config, Error};

#[tracing::instrument(level = "trace", skip(ctx))]
pub(crate) fn package(ctx: &Context) -> crate::Result<Vec<PathBuf>> {
    let Context { config, .. } = ctx;

    let out_dir = config.out_dir();
    let package_base_name = format!(
        "{}_{}_{}",
        config.product_name,
        config.version,
        match config.target_arch()? {
            "x86_64" => "x64",
            other => other,
        }
    );
    let app_bundle_file_name = format!("{}.app", config.product_name);
    let dmg_name = format!("{}.dmg", &package_base_name);
    let dmg_path = out_dir.join(&dmg_name);

    tracing::info!("Packaging {} ({})", dmg_name, dmg_path.display());

    if dmg_path.exists() {
        fs::remove_file(&dmg_path).map_err(|e| Error::IoWithPath(dmg_path.clone(), e))?;
    }

    let app_bundle_path = out_dir.join(&app_bundle_file_name);
    if !app_bundle_path.exists() {
        return Err(crate::Error::DoesNotExist(app_bundle_path));
    }

    // Calculate size: 1.5x app size + 200MB headroom. CEF-bearing bundles
    // contain many small helper bundles + a >700MB framework; HFS+ allocation
    // overhead easily eats the previous 20MB headroom and produces ENOSPC
    // partway through the copy.
    let app_size_bytes = dir_size(&app_bundle_path);
    let app_size_bytes = if app_size_bytes == 0 {
        600_000_000
    } else {
        app_size_bytes
    };
    let dmg_size_kb = ((app_size_bytes / 1024) * 3 / 2) + 204_800;

    detach_stale_volumes(&config.product_name);
    let sparse = MountedSparseImage::create_and_mount(&config.product_name, dmg_size_kb)?;

    tracing::debug!("Copying app bundle to DMG volume");
    let dest_app = PathBuf::from(&sparse.mount_dir).join(&app_bundle_file_name);
    copy_dir_recursive(&app_bundle_path, &dest_app)?;
    std::os::unix::fs::symlink(
        "/Applications",
        PathBuf::from(&sparse.mount_dir).join("Applications"),
    )
    .map_err(Error::Io)?;

    configure_finder_layout(&sparse.mount_dir, &app_bundle_file_name, config)?;
    stage_and_build_dmg(&sparse, &config.product_name, &dmg_path)?;
    sign_dmg(config, &dmg_path)?;

    Ok(vec![dmg_path])
}

struct DmgLayout {
    app_x: u32,
    app_y: u32,
    app_folder_x: u32,
    app_folder_y: u32,
    window_width: u32,
    window_height: u32,
    window_x: u32,
    window_y: u32,
}

impl DmgLayout {
    fn from_config(config: &Config) -> Self {
        let dmg = config.dmg();
        Self {
            app_x: dmg.and_then(|d| d.app_position).map(|p| p.x).unwrap_or(180),
            app_y: dmg.and_then(|d| d.app_position).map(|p| p.y).unwrap_or(170),
            app_folder_x: dmg
                .and_then(|d| d.app_folder_position)
                .map(|p| p.x)
                .unwrap_or(480),
            app_folder_y: dmg
                .and_then(|d| d.app_folder_position)
                .map(|p| p.y)
                .unwrap_or(170),
            window_width: dmg
                .and_then(|d| d.window_size)
                .map(|s| s.width)
                .unwrap_or(600),
            window_height: dmg
                .and_then(|d| d.window_size)
                .map(|s| s.height)
                .unwrap_or(400),
            window_x: dmg
                .and_then(|d| d.window_position)
                .map(|p| p.x)
                .unwrap_or(200),
            window_y: dmg
                .and_then(|d| d.window_position)
                .map(|p| p.y)
                .unwrap_or(120),
        }
    }

    fn applescript(
        &self,
        vol_name: &str,
        app_bundle_file_name: &str,
        background_script: &str,
    ) -> String {
        format!(
            r#"
tell application "Finder"
    tell disk "{vol_name}"
        open
        set current view of container window to icon view
        set toolbar visible of container window to false
        set statusbar visible of container window to false
        set the bounds of container window to {{{window_x}, {window_y}, {wx2}, {wy2}}}
        set viewOptions to the icon view options of container window
        set arrangement of viewOptions to not arranged
        set icon size of viewOptions to 100
        {background_script}
        set position of item "{app_bundle_file_name}" of container window to {{{app_x}, {app_y}}}
        set position of item "Applications" of container window to {{{app_folder_x}, {app_folder_y}}}
        close
        open
        update without registering applications
        delay 2
        close
    end tell
end tell
"#,
            vol_name = vol_name,
            window_x = self.window_x,
            window_y = self.window_y,
            wx2 = self.window_x + self.window_width,
            wy2 = self.window_y + self.window_height,
            background_script = background_script,
            app_bundle_file_name = app_bundle_file_name,
            app_x = self.app_x,
            app_y = self.app_y,
            app_folder_x = self.app_folder_x,
            app_folder_y = self.app_folder_y,
        )
    }
}

struct MountedSparseImage {
    rw_dmg_path: PathBuf,
    rw_dmg_base: PathBuf,
    device: String,
    mount_dir: String,
}

impl MountedSparseImage {
    fn create_and_mount(product_name: &str, dmg_size_kb: u64) -> crate::Result<Self> {
        tracing::debug!("Creating sparse HFS+ image");
        let rw_dmg = tempfile::NamedTempFile::new().map_err(Error::Io)?;
        let rw_dmg_path = rw_dmg.path().with_extension("sparseimage");
        let rw_dmg_base = rw_dmg.path().to_path_buf();
        drop(rw_dmg);

        Command::new("hdiutil")
            .args([
                "create",
                "-size",
                &format!("{}k", dmg_size_kb),
                "-fs",
                "HFS+",
                "-volname",
                product_name,
                "-type",
                "SPARSE",
            ])
            .arg(&rw_dmg_base)
            .output_ok()
            .map_err(crate::Error::CreateDmgFailed)?;

        tracing::debug!("Mounting sparse image");
        let mount_output = Command::new("hdiutil")
            .args(["attach", "-readwrite", "-noverify", "-noautoopen"])
            .arg(&rw_dmg_path)
            .output()
            .map_err(Error::Io)?;

        let mount_stdout = String::from_utf8_lossy(&mount_output.stdout);
        let device = mount_stdout
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().next())
            .ok_or_else(|| {
                crate::Error::CreateDmgFailed(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Failed to parse hdiutil attach output",
                ))
            })?
            .to_string();

        let mount_dir = mount_stdout
            .lines()
            .filter_map(|l| l.find("/Volumes/").map(|idx| l[idx..].to_string()))
            .next()
            .ok_or_else(|| {
                crate::Error::CreateDmgFailed(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Failed to find mount point in hdiutil attach output",
                ))
            })?;

        Ok(Self {
            rw_dmg_path,
            rw_dmg_base,
            device,
            mount_dir,
        })
    }

    fn detach_and_cleanup(&self) {
        let _ = Command::new("hdiutil")
            .args(["detach", &self.device, "-quiet"])
            .output();
        let _ = fs::remove_file(&self.rw_dmg_path);
        let _ = fs::remove_file(&self.rw_dmg_base);
    }
}

fn detach_stale_volumes(product_name: &str) {
    // Detach any stale `/Volumes/<product_name>*` mounts from interrupted
    // earlier runs so the new image mounts at the expected path. macOS
    // appends " 2", " 3", ... to disambiguate when the desired name is
    // taken, which would break the AppleScript layout step and copy paths.
    let Ok(info) = Command::new("hdiutil").arg("info").output() else {
        return;
    };
    let stdout = String::from_utf8_lossy(&info.stdout);
    let prefix = format!("/Volumes/{}", product_name);
    let prefix_with_space = format!("{} ", prefix);
    for line in stdout.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("/dev/") {
            continue;
        }
        let Some(mount_idx) = line.find("/Volumes/") else {
            continue;
        };
        let mount = line[mount_idx..].trim_end();
        if mount != prefix && !mount.starts_with(&prefix_with_space) {
            continue;
        }
        if let Some(dev) = trimmed.split_whitespace().next() {
            let _ = Command::new("hdiutil").args(["detach", dev]).output();
        }
    }
}

fn configure_finder_layout(
    mount_dir: &str,
    app_bundle_file_name: &str,
    config: &Config,
) -> crate::Result<()> {
    tracing::debug!("Configuring Finder layout via AppleScript");
    let vol_name = PathBuf::from(mount_dir)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let background_script = if let Some(bg_path) = config.dmg().and_then(|d| d.background.as_ref())
    {
        let bg_src = std::env::current_dir()?.join(bg_path);
        let bg_dir = PathBuf::from(mount_dir).join(".background");
        fs::create_dir_all(&bg_dir).map_err(|e| Error::IoWithPath(bg_dir.clone(), e))?;
        let bg_dest = bg_dir.join("background.png");
        fs::copy(&bg_src, &bg_dest).map_err(|e| Error::CopyFile(bg_src, bg_dest, e))?;
        "set background picture of viewOptions to file \".background:background.png\"".to_string()
    } else {
        "set background color of viewOptions to {7710, 7710, 7710}".to_string()
    };

    let layout = DmgLayout::from_config(config);
    let applescript = layout.applescript(&vol_name, app_bundle_file_name, &background_script);

    // Skip AppleScript in CI (no GUI) or if it fails (non-fatal)
    if std::env::var_os("CI").is_none() {
        let _ = Command::new("osascript")
            .arg("-e")
            .arg(&applescript)
            .output();
    }

    tracing::debug!("Cleaning up volume metadata");
    let fseventsd = PathBuf::from(mount_dir).join(".fseventsd");
    if fseventsd.exists() {
        let _ = fs::remove_dir_all(&fseventsd);
    }
    let bg_dir = PathBuf::from(mount_dir).join(".background");
    if bg_dir.exists() {
        let _ = Command::new("chflags").arg("hidden").arg(&bg_dir).output();
    }
    let _ = Command::new("sync").output();
    std::thread::sleep(std::time::Duration::from_secs(2));
    Ok(())
}

fn stage_and_build_dmg(
    sparse: &MountedSparseImage,
    product_name: &str,
    dmg_path: &Path,
) -> crate::Result<()> {
    // Stage the laid-out volume to a regular tempdir so the final -srcfolder
    // includes hidden files like .DS_Store and .background.
    tracing::debug!("Staging laid-out volume to tempdir");
    let stage_tmp = tempfile::Builder::new()
        .prefix("cargo-packager-dmg-stage")
        .tempdir()
        .map_err(Error::Io)?;
    let stage_dir = stage_tmp.path();
    Command::new("ditto")
        .arg(&sparse.mount_dir)
        .arg(stage_dir)
        .output_ok()
        .map_err(crate::Error::CreateDmgFailed)?;

    sparse.detach_and_cleanup();

    tracing::debug!("Creating compressed ULFO DMG from staged tempdir");
    Command::new("hdiutil")
        .args(["create", "-srcfolder"])
        .arg(stage_dir)
        .args([
            "-fs",
            "HFS+",
            "-volname",
            product_name,
            "-format",
            "ULFO",
            "-ov",
        ])
        .arg(dmg_path)
        .output_ok()
        .map_err(crate::Error::CreateDmgFailed)?;
    drop(stage_tmp);
    Ok(())
}

fn sign_dmg(config: &Config, dmg_path: &Path) -> crate::Result<()> {
    if let Some(identity) = config
        .macos()
        .and_then(|macos| macos.signing_identity.as_ref())
    {
        tracing::debug!("Codesigning {}", dmg_path.display());
        codesign::try_sign(
            vec![codesign::SignTarget {
                path: dmg_path.to_path_buf(),
                is_native_binary: false,
            }],
            identity,
            config,
        )?;
    }
    Ok(())
}

fn dir_size(path: &std::path::Path) -> u64 {
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.metadata().ok().map(|m| m.len()))
        .sum()
}

fn copy_dir_recursive(from: &std::path::Path, to: &std::path::Path) -> crate::Result<()> {
    fs::create_dir_all(to).map_err(|e| Error::IoWithPath(to.to_path_buf(), e))?;
    for entry in walkdir::WalkDir::new(from) {
        let entry = entry?;
        let path = entry.path();
        let rel_path = path.strip_prefix(from)?;
        let dest = to.join(rel_path);
        if entry.file_type().is_symlink() {
            let target =
                fs::read_link(path).map_err(|e| Error::IoWithPath(path.to_path_buf(), e))?;
            std::os::unix::fs::symlink(&target, &dest)
                .map_err(|e| Error::Symlink(target, dest, e))?;
        } else if entry.file_type().is_dir() {
            fs::create_dir_all(&dest).map_err(|e| Error::IoWithPath(dest, e))?;
        } else {
            fs::copy(path, &dest).map_err(|e| Error::CopyFile(path.to_path_buf(), dest, e))?;
        }
    }
    Ok(())
}
