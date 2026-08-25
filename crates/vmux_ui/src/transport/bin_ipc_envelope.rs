pub struct BinIpcEnvelope(Vec<u8>);

impl BinIpcEnvelope {
    pub const MAGIC: &'static [u8] = b"vmux-bin-ipc-v1\0";

    pub fn new(id: &str, payload: &[u8]) -> Self {
        let id_bytes = id.as_bytes();
        let id_len = u32::try_from(id_bytes.len()).expect("bin ipc id too long");
        let mut encoded =
            Vec::with_capacity(Self::MAGIC.len() + 4 + id_bytes.len() + payload.len());
        encoded.extend_from_slice(Self::MAGIC);
        encoded.extend_from_slice(&id_len.to_le_bytes());
        encoded.extend_from_slice(id_bytes);
        encoded.extend_from_slice(payload);
        Self(encoded)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn decode(bytes: &[u8]) -> Option<(String, Vec<u8>)> {
        let id_len_start = Self::MAGIC.len();
        let id_start = id_len_start + 4;
        if bytes.len() < id_start || !bytes.starts_with(Self::MAGIC) {
            return None;
        }
        let id_len = u32::from_le_bytes(bytes[id_len_start..id_start].try_into().ok()?) as usize;
        let payload_start = id_start.checked_add(id_len)?;
        if bytes.len() < payload_start {
            return None;
        }
        let id = std::str::from_utf8(&bytes[id_start..payload_start])
            .ok()?
            .to_string();
        Some((id, bytes[payload_start..].to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bin_ipc_envelope_preserves_id_and_payload_in_single_buffer() {
        let id = "vmux_command::event::CommandBarActionEvent";
        let payload = [1, 2, 3, 4];

        let envelope = BinIpcEnvelope::new(id, &payload);
        let encoded = envelope.as_bytes();
        let id_len_start = BinIpcEnvelope::MAGIC.len();
        let id_start = id_len_start + 4;
        let payload_start = id_start + id.len();
        let id_len = u32::from_le_bytes(
            encoded[id_len_start..id_start]
                .try_into()
                .expect("id len bytes"),
        );

        assert!(encoded.starts_with(BinIpcEnvelope::MAGIC));
        assert_eq!(id_len, id.len() as u32);
        assert_eq!(&encoded[id_start..payload_start], id.as_bytes());
        assert_eq!(&encoded[payload_start..], payload);
    }

    #[test]
    fn decode_recovers_what_new_framed_and_rejects_anything_else() {
        let id = "vmux_command::event::CommandBarActionEvent";
        let envelope = BinIpcEnvelope::new(id, &[1, 2, 3, 4]);

        let (decoded_id, payload) = BinIpcEnvelope::decode(envelope.as_bytes()).expect("envelope");

        assert_eq!(decoded_id, id);
        assert_eq!(payload, vec![1, 2, 3, 4]);
        assert!(BinIpcEnvelope::decode(&[1, 2, 3]).is_none());
        assert!(BinIpcEnvelope::decode(&envelope.as_bytes()[..8]).is_none());
    }
}
