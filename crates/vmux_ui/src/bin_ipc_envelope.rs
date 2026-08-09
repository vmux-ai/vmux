pub(crate) const BIN_IPC_ENVELOPE_MAGIC: &[u8] = b"vmux-bin-ipc-v1\0";

pub(crate) fn encode_bin_ipc_envelope(id: &str, payload: &[u8]) -> Vec<u8> {
    let id_bytes = id.as_bytes();
    let id_len = u32::try_from(id_bytes.len()).expect("bin ipc id too long");
    let mut encoded =
        Vec::with_capacity(BIN_IPC_ENVELOPE_MAGIC.len() + 4 + id_bytes.len() + payload.len());
    encoded.extend_from_slice(BIN_IPC_ENVELOPE_MAGIC);
    encoded.extend_from_slice(&id_len.to_le_bytes());
    encoded.extend_from_slice(id_bytes);
    encoded.extend_from_slice(payload);
    encoded
}

#[cfg(test)]
#[path = "bin_ipc_envelope.test.rs"]
mod tests;
