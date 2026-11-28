use std::fmt::{self, Write};
#[cfg(not(feature = "headless-host"))]
use std::path::Component;
use std::path::{Path, PathBuf};

use bevy_ecs::prelude::*;
use bevy_reflect::{Reflect, std_traits::ReflectDefault};

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Reflect)]
#[reflect(Component, Default)]
#[type_path = "vmux_ecs"]
pub struct FilePath(PathBuf);

impl FilePath {
    #[cfg(feature = "headless-host")]
    pub fn resolve(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        if let Ok(canonical) = path.canonicalize() {
            return Self(canonical);
        }
        let lexical = vmux_path::PathIdentity::lexical(path);
        Self(vmux_path::PathIdentity::resolve(lexical).into_path_buf())
    }

    #[cfg(not(feature = "headless-host"))]
    pub fn resolve(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        if let Ok(canonical) = path.canonicalize() {
            return Self(canonical);
        }
        let normalized = Self::normalize_lexical(path);
        if let Ok(canonical) = normalized.canonicalize() {
            return Self(canonical);
        }
        if let (Some(parent), Some(name)) = (normalized.parent(), normalized.file_name())
            && let Ok(canonical_parent) = parent.canonicalize()
        {
            return Self(canonical_parent.join(name));
        }
        Self(normalized)
    }

    #[cfg(feature = "headless-host")]
    pub fn lexical(path: impl AsRef<Path>) -> Self {
        Self(vmux_path::PathIdentity::lexical(path).into_path_buf())
    }

    #[cfg(not(feature = "headless-host"))]
    pub fn lexical(path: impl AsRef<Path>) -> Self {
        Self(Self::normalize_lexical(path.as_ref()))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }

    #[cfg(not(feature = "headless-host"))]
    fn normalize_lexical(path: &Path) -> PathBuf {
        let mut normalized = PathBuf::new();
        for component in path.components() {
            match component {
                Component::CurDir => {}
                Component::ParentDir => match normalized.components().next_back() {
                    Some(Component::Normal(_)) => {
                        normalized.pop();
                    }
                    Some(Component::ParentDir) | None if !path.is_absolute() => {
                        normalized.push("..");
                    }
                    _ => {}
                },
                _ => normalized.push(component.as_os_str()),
            }
        }
        normalized
    }
}

impl AsRef<Path> for FilePath {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

impl From<&Path> for FilePath {
    fn from(path: &Path) -> Self {
        Self::resolve(path)
    }
}

impl From<PathBuf> for FilePath {
    fn from(path: PathBuf) -> Self {
        Self::resolve(path)
    }
}

impl From<&str> for FilePath {
    fn from(path: &str) -> Self {
        Self::resolve(path)
    }
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Reflect)]
#[reflect(Component, Default)]
#[type_path = "vmux_ecs"]
pub struct ContentDigest(String);

impl ContentDigest {
    const SHA256_INIT: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    const SHA256_K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    pub fn sha256(bytes: &[u8]) -> Self {
        let mut state = Self::SHA256_INIT;
        let bit_len = (bytes.len() as u64).wrapping_mul(8);
        let mut padded = Vec::with_capacity((bytes.len() + 9).div_ceil(64) * 64);
        padded.extend_from_slice(bytes);
        padded.push(0x80);
        while padded.len() % 64 != 56 {
            padded.push(0);
        }
        padded.extend_from_slice(&bit_len.to_be_bytes());
        for block in padded.chunks_exact(64) {
            let mut words = [0u32; 64];
            for (index, chunk) in block.chunks_exact(4).enumerate() {
                words[index] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            }
            for index in 16..64 {
                let s0 = words[index - 15].rotate_right(7)
                    ^ words[index - 15].rotate_right(18)
                    ^ (words[index - 15] >> 3);
                let s1 = words[index - 2].rotate_right(17)
                    ^ words[index - 2].rotate_right(19)
                    ^ (words[index - 2] >> 10);
                words[index] = words[index - 16]
                    .wrapping_add(s0)
                    .wrapping_add(words[index - 7])
                    .wrapping_add(s1);
            }
            let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
            for (&word, &constant) in words.iter().zip(&Self::SHA256_K) {
                let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
                let choice = (e & f) ^ ((!e) & g);
                let temp1 = h
                    .wrapping_add(s1)
                    .wrapping_add(choice)
                    .wrapping_add(constant)
                    .wrapping_add(word);
                let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
                let majority = (a & b) ^ (a & c) ^ (b & c);
                let temp2 = s0.wrapping_add(majority);
                h = g;
                g = f;
                f = e;
                e = d.wrapping_add(temp1);
                d = c;
                c = b;
                b = a;
                a = temp1.wrapping_add(temp2);
            }
            for (slot, delta) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
                *slot = slot.wrapping_add(delta);
            }
        }
        let mut encoded = String::with_capacity(7 + 64);
        encoded.push_str("sha256:");
        for word in state {
            let _ = write!(&mut encoded, "{word:08x}");
        }
        Self(encoded)
    }

    pub fn from_file(path: impl AsRef<Path>) -> std::io::Result<Self> {
        crate::resource_path_io::ContentDigestIo::from_file(path)
    }

    pub fn parse(raw: &str) -> Result<Self, String> {
        let trimmed = raw.trim();
        let Some((algorithm, hex)) = trimmed.split_once(':') else {
            return Err("content digest must use <algorithm>:<hex> format".into());
        };
        if algorithm.is_empty()
            || !algorithm
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err("content digest algorithm is invalid".into());
        }
        if hex.is_empty() || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("content digest hex payload is invalid".into());
        }
        Ok(Self(format!(
            "{}:{}",
            algorithm.to_ascii_lowercase(),
            hex.to_ascii_lowercase()
        )))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn algorithm(&self) -> Option<&str> {
        self.0.split_once(':').map(|(algorithm, _)| algorithm)
    }

    pub fn hex(&self) -> Option<&str> {
        self.0.split_once(':').map(|(_, hex)| hex)
    }

    pub fn matches_bytes(&self, bytes: &[u8]) -> bool {
        self.algorithm() == Some("sha256") && Self::sha256(bytes) == *self
    }
}

impl fmt::Display for ContentDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_path_normalizes_lexical_and_existing_filesystem_paths() {
        assert_eq!(FilePath::lexical("a/./b/../c").as_path(), Path::new("a/c"));
        assert_eq!(FilePath::lexical("../../a").as_path(), Path::new("../../a"));

        let directory = tempfile::tempdir().unwrap();
        let canonical_dir = directory.path().canonicalize().unwrap();
        let existing = directory.path().join("note.md");
        std::fs::write(&existing, b"hello").unwrap();

        assert_eq!(
            FilePath::resolve(&existing).as_path(),
            canonical_dir.join("note.md")
        );
        assert_eq!(
            FilePath::resolve(directory.path().join("missing.md")).as_path(),
            canonical_dir.join("missing.md")
        );
    }

    #[test]
    fn content_digest_matches_standard_sha256_vectors_and_files() {
        assert_eq!(
            ContentDigest::sha256(b"").as_str(),
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            ContentDigest::sha256(b"abc").as_str(),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            ContentDigest::sha256(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")
                .as_str(),
            "sha256:248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );

        let parsed = ContentDigest::parse("SHA256:BA7816BF").unwrap();
        assert_eq!(parsed.algorithm(), Some("sha256"));
        assert_eq!(parsed.hex(), Some("ba7816bf"));
        assert!(ContentDigest::parse("missing-colon").is_err());
        assert!(ContentDigest::parse("sha256:not-hex").is_err());

        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("sample.txt");
        std::fs::write(&file, b"abc").unwrap();
        let digest = ContentDigest::from_file(&file).unwrap();
        assert!(digest.matches_bytes(b"abc"));
        assert!(!digest.matches_bytes(b"abd"));
    }
}
