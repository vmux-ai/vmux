use std::path::Path;

use crate::resource_path::ContentDigest;

pub(crate) struct ContentDigestIo;

impl ContentDigestIo {
    pub(crate) fn from_file(path: impl AsRef<Path>) -> std::io::Result<ContentDigest> {
        let bytes = std::fs::read(path)?;
        Ok(ContentDigest::sha256(&bytes))
    }
}
