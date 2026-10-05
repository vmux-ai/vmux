use std::io::{Read, Write};
use std::path::Path;

pub(crate) struct Downloader;

impl Downloader {
    pub(crate) fn fetch(
        url: &str,
        destination: &Path,
        mut progress: impl FnMut(u64, Option<u64>),
    ) -> Result<(), String> {
        let mut response = reqwest::blocking::get(url).map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Err(format!("http {}", response.status()));
        }
        let total = response.content_length();
        let mut file = std::fs::File::create(destination).map_err(|error| error.to_string())?;
        let mut buffer = [0u8; 8192];
        let mut received = 0u64;
        loop {
            let count = response
                .read(&mut buffer)
                .map_err(|error| error.to_string())?;
            if count == 0 {
                break;
            }
            file.write_all(&buffer[..count])
                .map_err(|error| error.to_string())?;
            received += count as u64;
            progress(received, total);
        }
        Ok(())
    }
}
