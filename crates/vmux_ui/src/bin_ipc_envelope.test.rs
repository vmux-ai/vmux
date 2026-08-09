use super::*;

#[test]
fn bin_ipc_envelope_preserves_id_and_payload_in_single_buffer() {
    let id = "vmux_command::event::CommandBarActionEvent";
    let payload = [1, 2, 3, 4];

    let encoded = encode_bin_ipc_envelope(id, &payload);
    let id_len_start = BIN_IPC_ENVELOPE_MAGIC.len();
    let id_start = id_len_start + 4;
    let payload_start = id_start + id.len();
    let id_len = u32::from_le_bytes(
        encoded[id_len_start..id_start]
            .try_into()
            .expect("id len bytes"),
    );

    assert!(encoded.starts_with(BIN_IPC_ENVELOPE_MAGIC));
    assert_eq!(id_len, id.len() as u32);
    assert_eq!(&encoded[id_start..payload_start], id.as_bytes());
    assert_eq!(&encoded[payload_start..], payload);
}
