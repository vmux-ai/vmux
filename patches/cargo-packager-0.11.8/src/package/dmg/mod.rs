// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// Copyright 2023-2023 CrabNebula Ltd.
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT
//
// Patched: replaced create-dmg script with direct hdiutil approach to work
// around macOS 26 Tahoe `hdiutil convert` mandatory file locking regression.

use std::{fs, path::PathBuf, process::Command};

use super::Context;
use crate::{codesign::macos as codesign, shell::CommandExt, Error};

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

    let dmg = config.dmg();

    // Read DMG layout config
    let app_x = dmg
        .and_then(|d| d.app_position)
        .map(|p| p.x)
        .unwrap_or(180);
    let app_y = dmg
        .and_then(|d| d.app_position)
        .map(|p| p.y)
        .unwrap_or(170);
    let app_folder_x = dmg
        .and_then(|d| d.app_folder_position)
        .map(|p| p.x)
        .unwrap_or(480);
    let app_folder_y = dmg
        .and_then(|d| d.app_folder_position)
        .map(|p| p.y)
        .unwrap_or(170);
    let window_width = dmg
        .and_then(|d| d.window_size)
        .map(|s| s.width)
        .unwrap_or(600);
    let window_height = dmg
        .and_then(|d| d.window_size)
        .map(|s| s.height)
        .unwrap_or(400);
    let window_x = dmg
        .and_then(|d| d.window_position)
        .map(|p| p.x)
        .unwrap_or(200);
    let window_y = dmg
        .and_then(|d| d.window_position)
        .map(|p| p.y)
        .unwrap_or(120);

    // Calculate size: app size + 20MB headroom
    let app_size_bytes = dir_size(&app_bundle_path);
    let app_size_bytes = if app_size_bytes == 0 { 600_000_000 } else { app_size_bytes };
    let dmg_size_kb = (app_size_bytes / 1024) + 20480;

    // 1. Create empty read-write HFS+ sparse image
    tracing::debug!("Creating sparse HFS+ image");
    let rw_dmg = tempfile::NamedTempFile::new()
        .map_err(|e| Error::Io(e))?;
    let rw_dmg_path = rw_dmg.path().with_extension("sparseimage");
    let rw_dmg_base = rw_dmg.path().to_path_buf();
    drop(rw_dmg); // release the temp file, we just need the path

    Command::new("hdiutil")
        .args([
            "create",
            "-size",
            &format!("{}k", dmg_size_kb),
            "-fs",
            "HFS+",
            "-volname",
            &config.product_name,
            "-type",
            "SPARSE",
        ])
        .arg(&rw_dmg_base)
        .output_ok()
        .map_err(crate::Error::CreateDmgFailed)?;

    // 2. Mount it
    tracing::debug!("Mounting sparse image");
    let mount_output = Command::new("hdiutil")
        .args(["attach", "-readwrite", "-noverify", "-noautoopen"])
        .arg(&rw_dmg_path)
        .output()
        .map_err(|e| Error::Io(e))?;

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
        .filter_map(|l| {
            if let Some(idx) = l.find("/Volumes/") {
                Some(l[idx..].to_string())
            } else {
                None
            }
        })
        .next()
        .ok_or_else(|| {
            crate::Error::CreateDmgFailed(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to find mount point in hdiutil attach output",
            ))
        })?;

    // 3. Copy contents onto the mounted volume
    tracing::debug!("Copying app bundle to DMG volume");
    let dest_app = PathBuf::from(&mount_dir).join(&app_bundle_file_name);
    copy_dir_recursive(&app_bundle_path, &dest_app)?;

    std::os::unix::fs::symlink("/Applications", PathBuf::from(&mount_dir).join("Applications"))
        .map_err(|e| Error::Io(e))?;

    // 4. Configure Finder layout via AppleScript
    tracing::debug!("Configuring Finder layout via AppleScript");
    let vol_name = PathBuf::from(&mount_dir)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let background_script = if let Some(bg_path) = dmg.and_then(|d| d.background.as_ref()) {
        let bg_src = std::env::current_dir()?.join(bg_path);
        let bg_dir = PathBuf::from(&mount_dir).join(".background");
        fs::create_dir_all(&bg_dir).map_err(|e| Error::IoWithPath(bg_dir.clone(), e))?;
        let bg_dest = bg_dir.join("background.png");
        fs::copy(&bg_src, &bg_dest).map_err(|e| Error::CopyFile(bg_src, bg_dest, e))?;
        format!(
            "set background picture of viewOptions to file \".background:background.png\""
        )
    } else {
        "set background color of viewOptions to {7710, 7710, 7710}".to_string()
    };

    let applescript = format!(
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
        window_x = window_x,
        window_y = window_y,
        wx2 = window_x + window_width as u32,
        wy2 = window_y + window_height as u32,
        background_script = background_script,
        app_bundle_file_name = app_bundle_file_name,
        app_x = app_x,
        app_y = app_y,
        app_folder_x = app_folder_x,
        app_folder_y = app_folder_y,
    );

    // Skip AppleScript in CI (no GUI) or if it fails (non-fatal)
    if std::env::var_os("CI").is_none() {
        let _ = Command::new("osascript")
            .arg("-e")
            .arg(&applescript)
            .output();
    }

    // 5. Clean up and flush
    tracing::debug!("Cleaning up volume metadata");
    let fseventsd = PathBuf::from(&mount_dir).join(".fseventsd");
    if fseventsd.exists() {
        let _ = fs::remove_dir_all(&fseventsd);
    }
    if let Some(bg_dir) = Some(PathBuf::from(&mount_dir).join(".background")) {
        if bg_dir.exists() {
            let _ = Command::new("chflags")
                .arg("hidden")
                .arg(&bg_dir)
                .output();
        }
    }
    let _ = Command::new("sync").output();
    std::thread::sleep(std::time::Duration::from_secs(2));

    // 6. Create compressed read-only DMG from mounted device partition
    //    Uses -srcdevice instead of hdiutil convert to avoid macOS 26 Tahoe
    //    mandatory file locking regression (EAGAIN on output file).
    tracing::debug!("Creating compressed UDZO DMG from device");
    let diskutil_output = Command::new("diskutil")
        .args(["list", &device])
        .output()
        .map_err(|e| Error::Io(e))?;

    let diskutil_stdout = String::from_utf8_lossy(&diskutil_output.stdout);
    let partition = diskutil_stdout
        .lines()
        .find(|l| l.contains("Apple_HFS"))
        .and_then(|l| l.split_whitespace().last())
        .ok_or_else(|| {
            crate::Error::CreateDmgFailed(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to find HFS+ partition in diskutil output",
            ))
        })?;

    Command::new("hdiutil")
        .args([
            "create",
            "-srcdevice",
            &format!("/dev/{}", partition),
            "-format",
            "UDZO",
            "-ov",
            "-o",
        ])
        .arg(&dmg_path)
        .output_ok()
        .map_err(crate::Error::CreateDmgFailed)?;

    // 7. Detach and clean up
    tracing::debug!("Detaching and cleaning up");
    let _ = Command::new("hdiutil")
        .args(["detach", &device, "-quiet"])
        .output();
    let _ = fs::remove_file(&rw_dmg_path);
    let _ = fs::remove_file(&rw_dmg_base);

    // Sign DMG if needed
    if let Some(identity) = &config
        .macos()
        .and_then(|macos| macos.signing_identity.as_ref())
    {
        tracing::debug!("Codesigning {}", dmg_path.display());
        codesign::try_sign(
            vec![codesign::SignTarget {
                path: dmg_path.clone(),
                is_native_binary: false,
            }],
            identity,
            config,
        )?;
    }

    Ok(vec![dmg_path])
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
            fs::copy(path, &dest)
                .map_err(|e| Error::CopyFile(path.to_path_buf(), dest, e))?;
        }
    }
    Ok(())
}
