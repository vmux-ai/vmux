//! Hex decoding for keys handed between the app and the macOS key broker over a
//! pipe. Only the macOS path uses it, but it is compiled under `test` on every
//! platform so CI exercises the encoding away from the Keychain.

use crate::vault::{KEY_LEN, validate_key};

pub(super) fn decode_key_hex(value: &str) -> Result<Vec<u8>, String> {
    if value.len() != KEY_LEN * 2 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Vault encryption key has an invalid encoding".to_string());
    }
    let bytes = value.as_bytes();
    let mut key = Vec::with_capacity(KEY_LEN);
    for pair in bytes.chunks_exact(2) {
        let high = hex_value(pair[0])?;
        let low = hex_value(pair[1])?;
        key.push((high << 4) | low);
    }
    validate_key(&key)?;
    Ok(key)
}

fn hex_value(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err("Vault encryption key has an invalid encoding".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::hex;

    #[test]
    fn vault_key_broker_encoding_round_trips() {
        let key = (0..KEY_LEN).map(|value| value as u8).collect::<Vec<_>>();

        assert_eq!(decode_key_hex(&hex(&key)), Ok(key));
        assert!(decode_key_hex("00").is_err());
        assert!(decode_key_hex(&"z".repeat(KEY_LEN * 2)).is_err());
    }
}
