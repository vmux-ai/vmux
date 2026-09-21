use ring::{aead, hkdf};
use zeroize::Zeroizing;

use super::{
    BROWSER_KEY_LENGTH, BrowserKey, DATA_KEY_LENGTH, DataKey, RootKey, SafeStorageError,
    VaultWrappingKey,
};

const NONCE_LENGTH: usize = 12;
const ENVELOPE_HEADER: &[u8] = b"vmux-safe-storage-envelope-v1\0";
const HKDF_SALT: &[u8] = b"vmux-safe-storage-hkdf-v1";
const BROWSER_CONTEXT: &[u8] = b"vmux-safe-storage-browser-v1";
const MCP_CONTEXT: &[u8] = b"vmux-safe-storage-mcp-v1";
const VAULT_CONTEXT: &[u8] = b"vmux-safe-storage-vault-wrap-v1";

pub(super) struct SafeStorageCipher {
    root: RootKey,
}

struct DerivedKeyLength(usize);

impl SafeStorageCipher {
    pub(super) fn new(root: RootKey) -> Self {
        Self { root }
    }

    pub(super) fn browser_key(&self) -> Result<BrowserKey, SafeStorageError> {
        let mut bytes = [0_u8; BROWSER_KEY_LENGTH];
        self.derive(BROWSER_CONTEXT, &[], &mut bytes)?;
        Ok(BrowserKey::new(bytes))
    }

    pub(super) fn seal_mcp(
        &self,
        account: &str,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, SafeStorageError> {
        let key = self.data_key(MCP_CONTEXT, &[])?;
        self.seal(key.as_bytes(), account.as_bytes(), plaintext)
    }

    pub(super) fn open_mcp(
        &self,
        account: &str,
        envelope: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, SafeStorageError> {
        let key = self.data_key(MCP_CONTEXT, &[])?;
        self.open(key.as_bytes(), account.as_bytes(), envelope)
    }

    pub(super) fn wrap_vault_key(
        &self,
        vault_id: &str,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, SafeStorageError> {
        let key = self.vault_wrapping_key(vault_id)?;
        self.seal(key.as_bytes(), vault_id.as_bytes(), plaintext)
    }

    pub(super) fn unwrap_vault_key(
        &self,
        vault_id: &str,
        envelope: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, SafeStorageError> {
        let key = self.vault_wrapping_key(vault_id)?;
        self.open(key.as_bytes(), vault_id.as_bytes(), envelope)
    }

    fn data_key(&self, label: &[u8], context: &[u8]) -> Result<DataKey, SafeStorageError> {
        let mut bytes = [0_u8; DATA_KEY_LENGTH];
        self.derive(label, context, &mut bytes)?;
        Ok(DataKey::new(bytes))
    }

    fn vault_wrapping_key(&self, vault_id: &str) -> Result<VaultWrappingKey, SafeStorageError> {
        let mut bytes = [0_u8; DATA_KEY_LENGTH];
        self.derive(VAULT_CONTEXT, vault_id.as_bytes(), &mut bytes)?;
        Ok(VaultWrappingKey::new(bytes))
    }

    fn derive(
        &self,
        label: &[u8],
        context: &[u8],
        output: &mut [u8],
    ) -> Result<(), SafeStorageError> {
        let salt = hkdf::Salt::new(hkdf::HKDF_SHA256, HKDF_SALT);
        let prk = salt.extract(self.root.as_bytes());
        let info = [label, context];
        let output_key = prk
            .expand(&info, DerivedKeyLength(output.len()))
            .map_err(|_| SafeStorageError::Crypto("failed to derive Vmux Safe Storage key"))?;
        output_key
            .fill(output)
            .map_err(|_| SafeStorageError::Crypto("failed to derive Vmux Safe Storage key"))
    }

    fn seal(
        &self,
        key: &[u8; DATA_KEY_LENGTH],
        aad: &[u8],
        plaintext: &[u8],
    ) -> Result<Vec<u8>, SafeStorageError> {
        use ring::rand::SecureRandom;

        let key = aead::UnboundKey::new(&aead::AES_256_GCM, key).map_err(|_| {
            SafeStorageError::Crypto("failed to initialize Vmux Safe Storage encryption")
        })?;
        let key = aead::LessSafeKey::new(key);
        let mut nonce = [0_u8; NONCE_LENGTH];
        ring::rand::SystemRandom::new()
            .fill(&mut nonce)
            .map_err(|_| SafeStorageError::Crypto("failed to generate Vmux Safe Storage nonce"))?;
        let mut encrypted = plaintext.to_vec();
        key.seal_in_place_append_tag(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(aad),
            &mut encrypted,
        )
        .map_err(|_| SafeStorageError::Crypto("failed to encrypt Vmux Safe Storage data"))?;
        let mut envelope =
            Vec::with_capacity(ENVELOPE_HEADER.len() + nonce.len() + encrypted.len());
        envelope.extend_from_slice(ENVELOPE_HEADER);
        envelope.extend_from_slice(&nonce);
        envelope.extend_from_slice(&encrypted);
        Ok(envelope)
    }

    fn open(
        &self,
        key: &[u8; DATA_KEY_LENGTH],
        aad: &[u8],
        envelope: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, SafeStorageError> {
        let Some(envelope) = envelope.strip_prefix(ENVELOPE_HEADER) else {
            return Err(SafeStorageError::UnsupportedVersion);
        };
        if envelope.len() < NONCE_LENGTH + aead::AES_256_GCM.tag_len() {
            return Err(SafeStorageError::CorruptEnvelope);
        }
        let (nonce, ciphertext) = envelope.split_at(NONCE_LENGTH);
        let nonce: [u8; NONCE_LENGTH] = nonce
            .try_into()
            .map_err(|_| SafeStorageError::CorruptEnvelope)?;
        let key = aead::UnboundKey::new(&aead::AES_256_GCM, key).map_err(|_| {
            SafeStorageError::Crypto("failed to initialize Vmux Safe Storage decryption")
        })?;
        let key = aead::LessSafeKey::new(key);
        let mut plaintext = Zeroizing::new(ciphertext.to_vec());
        let length = key
            .open_in_place(
                aead::Nonce::assume_unique_for_key(nonce),
                aead::Aad::from(aad),
                &mut plaintext,
            )
            .map_err(|_| SafeStorageError::CorruptEnvelope)?
            .len();
        plaintext.truncate(length);
        Ok(plaintext)
    }
}

impl hkdf::KeyType for DerivedKeyLength {
    fn len(&self) -> usize {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safe_storage::ROOT_KEY_LENGTH;

    fn cipher() -> SafeStorageCipher {
        let mut root = [0_u8; ROOT_KEY_LENGTH];
        for (index, byte) in root.iter_mut().enumerate() {
            *byte = index as u8;
        }
        SafeStorageCipher::new(RootKey::new(root))
    }

    #[test]
    fn derived_keys_are_domain_separated() {
        let cipher = cipher();
        let browser = cipher.browser_key().unwrap();
        let mcp = cipher.data_key(MCP_CONTEXT, &[]).unwrap();
        let first_vault = cipher.vault_wrapping_key("first").unwrap();
        let second_vault = cipher.vault_wrapping_key("second").unwrap();

        assert_ne!(browser.as_bytes().as_slice(), mcp.as_bytes().as_slice());
        assert_ne!(mcp.as_bytes(), first_vault.as_bytes());
        assert_ne!(first_vault.as_bytes(), second_vault.as_bytes());
    }

    #[test]
    fn encrypted_data_requires_the_matching_domain_and_context() {
        let cipher = cipher();
        let encrypted = cipher.seal_mcp("personal:linear", b"secret").unwrap();

        assert_eq!(
            cipher
                .open_mcp("personal:linear", &encrypted)
                .unwrap()
                .as_slice(),
            b"secret"
        );
        assert!(cipher.open_mcp("personal:github", &encrypted).is_err());
        assert!(
            cipher
                .unwrap_vault_key("personal:linear", &encrypted)
                .is_err()
        );
    }

    #[test]
    fn encrypted_data_rejects_tampering() {
        let cipher = cipher();
        let mut encrypted = cipher.wrap_vault_key("vault", b"secret").unwrap();
        *encrypted.last_mut().unwrap() ^= 1;

        assert!(cipher.unwrap_vault_key("vault", &encrypted).is_err());
    }
}
