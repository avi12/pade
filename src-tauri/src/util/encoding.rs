//! Percent and hexadecimal encoding, one encoder for every `%XX` need.

/// Percent-encode `value` per RFC 3986: the unreserved set (`A–Z a–z 0–9 - _ . ~`)
/// and any bytes in `extra` stay literal, everything else becomes `%XX`. Callers
/// pass `extra` for characters a target must keep verbatim (e.g. `/` and `:` in a
/// URL path). One authoritative encoder (DRY) for every `%XX` need.
pub fn percent_encode(value: &str, extra: &[u8]) -> String {
    let mut output = String::with_capacity(value.len());
    for &byte in value.as_bytes() {
        let literal = byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'_' | b'.' | b'~')
            || extra.contains(&byte);
        if literal {
            output.push(char::from(byte));
        } else {
            output.push('%');
            output.push(char::from(hex_nibble(byte >> 4)));
            output.push(char::from(hex_nibble(byte & 0x0f)));
        }
    }
    output
}

/// Bytes as lowercase hex - the form digests are compared and written in.
pub fn lowercase_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        // Writing into a `String` cannot fail.
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// The uppercase hex character for a nibble (0..=15).
fn hex_nibble(nibble: u8) -> u8 {
    match nibble {
        0..=9 => b'0' + nibble,
        _ => b'A' + (nibble - 10),
    }
}
