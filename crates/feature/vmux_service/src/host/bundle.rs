use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppBundle(PathBuf);

impl AppBundle {
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl TryFrom<&Path> for AppBundle {
    type Error = ();

    fn try_from(exe: &Path) -> Result<Self, Self::Error> {
        let parent = exe.parent().ok_or(())?;
        if parent.file_name().and_then(|name| name.to_str()) != Some("MacOS") {
            return Err(());
        }
        let contents = parent.parent().ok_or(())?;
        if contents.file_name().and_then(|name| name.to_str()) != Some("Contents") {
            return Err(());
        }
        let app = contents.parent().ok_or(())?;
        if app.extension().and_then(|extension| extension.to_str()) != Some("app") {
            return Err(());
        }
        Ok(Self(app.to_path_buf()))
    }
}

pub const EMBEDDED_AGENT_PLIST: &str = "ai.vmux.service.plist";
