use super::{
    activity_payload, encode_frame, read_frame, ActivityFields, DiscordError, Opcode,
    FRAME_HEADER_LENGTH,
};

#[test]
fn frame_header_is_little_endian_opcode_then_length() {
    let payload: &[u8] = br#"{"v":1}"#;
    let frame = encode_frame(Opcode::Frame, payload).expect("small payload encodes");
    assert_eq!(&frame[0..4], &Opcode::Frame.code().to_le_bytes());
    let length = u32::try_from(payload.len()).expect("length fits");
    assert_eq!(&frame[4..8], &length.to_le_bytes());
    assert_eq!(&frame[FRAME_HEADER_LENGTH..], payload);
}

/// A reply as Discord frames it, followed by whatever comes after it.
fn framed(payload: &[u8], trailing: &[u8]) -> std::io::Cursor<Vec<u8>> {
    let mut bytes = encode_frame(Opcode::Frame, payload).expect("encodes");
    bytes.extend_from_slice(trailing);
    std::io::Cursor::new(bytes)
}

#[test]
fn reading_a_frame_drains_exactly_that_frame() {
    let mut stream = framed(br#"{"evt":"READY"}"#, b"next");
    read_frame(&mut stream).expect("a whole frame reads");
    let position = usize::try_from(stream.position()).expect("fits");
    assert_eq!(&stream.get_ref()[position..], b"next");
}

#[test]
fn a_truncated_or_oversized_frame_is_an_error() {
    let mut truncated = framed(b"0123456789", b"");
    truncated.get_mut().truncate(FRAME_HEADER_LENGTH + 4);
    assert!(matches!(
        read_frame(&mut truncated),
        Err(DiscordError::Io(_))
    ));

    let mut header = Opcode::Frame.code().to_le_bytes().to_vec();
    header.extend_from_slice(&u32::MAX.to_le_bytes());
    let mut oversized = std::io::Cursor::new(header);
    assert!(matches!(
        read_frame(&mut oversized),
        Err(DiscordError::OversizedFrame)
    ));
}

#[test]
fn every_ipc_socket_slot_is_tried_in_order() {
    let names: Vec<String> = super::ipc_socket_names().collect();
    assert_eq!(names.len(), 10);
    assert_eq!(names[0], "discord-ipc-0");
    assert_eq!(names[9], "discord-ipc-9");
}

#[test]
fn handshake_opcode_encodes_as_zero() {
    let frame = encode_frame(Opcode::Handshake, b"{}").expect("encodes");
    assert_eq!(&frame[0..4], &0u32.to_le_bytes());
}

#[test]
fn activity_payload_omits_absent_fields_and_carries_the_start_time() {
    let payload = activity_payload(ActivityFields {
        nonce: "nonce-1",
        details: Some("Working on pade"),
        state: None,
        image: None,
        caption: None,
        started_at: 1_700_000_000,
    });
    let value: serde_json::Value = serde_json::from_str(&payload).expect("valid json");
    assert_eq!(value["cmd"], serde_json::json!("SET_ACTIVITY"));
    let activity = &value["args"]["activity"];
    assert_eq!(activity["details"], serde_json::json!("Working on pade"));
    assert!(activity.get("state").is_none());
    assert_eq!(activity["assets"]["large_image"], serde_json::json!("pade"));
    // No language passed → no small overlay, just the PADE large asset.
    assert!(activity["assets"].get("small_image").is_none());
    assert_eq!(
        activity["timestamps"]["start"],
        serde_json::json!(1_700_000_000u64)
    );
}

#[test]
fn activity_payload_carries_the_language_overlay_when_given() {
    let payload = activity_payload(ActivityFields {
        nonce: "nonce-3",
        details: Some("Working on pade"),
        state: Some("Rust"),
        image: Some("rust"),
        caption: Some("Rust"),
        started_at: 1_700_000_000,
    });
    let value: serde_json::Value = serde_json::from_str(&payload).expect("valid json");
    let assets = &value["args"]["activity"]["assets"];
    assert_eq!(assets["large_image"], serde_json::json!("pade"));
    assert_eq!(assets["small_image"], serde_json::json!("rust"));
    assert_eq!(assets["small_text"], serde_json::json!("Rust"));
}

#[test]
fn clear_payload_sends_a_null_activity() {
    let payload = super::clear_payload("nonce-2");
    let value: serde_json::Value = serde_json::from_str(&payload).expect("valid json");
    assert_eq!(value["cmd"], serde_json::json!("SET_ACTIVITY"));
    assert_eq!(value["args"]["activity"], serde_json::Value::Null);
}
