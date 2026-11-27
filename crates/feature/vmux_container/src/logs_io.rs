use std::io::{self, Read, Seek, SeekFrom};

use vmux_vm::Settings;

pub(crate) struct EngineLogsIo;

impl EngineLogsIo {
    pub(crate) fn read(settings: &Settings) -> io::Result<String> {
        let mut logs = String::new();
        for path in [settings.log(), settings.directory.join("guest.log")] {
            let mut file = match std::fs::File::open(&path) {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            file.seek(SeekFrom::Start(
                file.metadata()?.len().saturating_sub(65536),
            ))?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            logs.push_str(&String::from_utf8_lossy(&bytes));
        }
        Ok(logs)
    }
}
