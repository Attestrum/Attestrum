//! In-tree hex encode/decode. We deliberately avoid the `hex` crate — these are
//! ~40 lines of trivial code, and Sprint 1's dep policy is "zero new deps".
//! The whole module is `pub` because hex helpers are useful from every other
//! Attestrum crate.

use crate::AttestrumError;

/// Lowercase hex-encode a byte slice. `b'\xde\xad'` → `"dead"`.
pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(nibble_to_hex(b >> 4));
        out.push(nibble_to_hex(b & 0x0f));
    }
    out
}

/// Decode a hex string into bytes. Even length and only `[0-9a-fA-F]` required.
pub fn decode(s: &str) -> Result<Vec<u8>, AttestrumError> {
    if s.len() % 2 != 0 {
        return Err(AttestrumError::Hash(format!(
            "hex string has odd length: {}",
            s.len()
        )));
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(s.len() / 2);
    for chunk in bytes.chunks_exact(2) {
        let hi = hex_to_nibble(chunk[0])?;
        let lo = hex_to_nibble(chunk[1])?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

/// Convenience wrapper for fixed-size 32-byte values (BLAKE3, SHA-256 digests).
pub fn encode_32(bytes: &[u8; 32]) -> String {
    encode(bytes)
}

/// Decode a 64-char hex string into a `[u8; 32]`. Wrong length → `AttestrumError::Hash`.
pub fn decode_32(s: &str) -> Result<[u8; 32], AttestrumError> {
    let v = decode(s)?;
    if v.len() != 32 {
        return Err(AttestrumError::Hash(format!(
            "expected 32-byte hex (64 chars), got {} bytes",
            v.len()
        )));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&v);
    Ok(arr)
}

fn nibble_to_hex(n: u8) -> char {
    debug_assert!(n < 16);
    if n < 10 {
        (b'0' + n) as char
    } else {
        (b'a' + n - 10) as char
    }
}

fn hex_to_nibble(c: u8) -> Result<u8, AttestrumError> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        other => Err(AttestrumError::Hash(format!(
            "invalid hex character: {:?}",
            other as char
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_basic() {
        assert_eq!(encode(&[0x00, 0xff, 0xde, 0xad]), "00ffdead");
        assert_eq!(encode(&[]), "");
    }

    #[test]
    fn decode_basic() {
        assert_eq!(decode("00ffdead").unwrap(), vec![0x00, 0xff, 0xde, 0xad]);
        assert_eq!(decode("").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn encode_decode_round_trip() {
        let original: Vec<u8> = (0..=255).collect();
        let s = encode(&original);
        let back = decode(&s).unwrap();
        assert_eq!(original, back);
    }

    #[test]
    fn case_insensitive_decode() {
        assert_eq!(decode("DeAdBeEf").unwrap(), vec![0xde, 0xad, 0xbe, 0xef]);
    }

    #[test]
    fn encode_32_decode_32_round_trip() {
        let mut original = [0u8; 32];
        for (i, b) in original.iter_mut().enumerate() {
            *b = i as u8;
        }
        let s = encode_32(&original);
        assert_eq!(s.len(), 64);
        let back = decode_32(&s).unwrap();
        assert_eq!(original, back);
    }

    #[test]
    fn decode_rejects_odd_length() {
        let err = decode("abc").unwrap_err();
        assert!(err.to_string().contains("odd length"), "got {err}");
    }

    #[test]
    fn decode_rejects_non_hex_chars() {
        let err = decode("zz").unwrap_err();
        assert!(err.to_string().contains("invalid hex"), "got {err}");
    }

    #[test]
    fn decode_32_rejects_wrong_length() {
        assert!(decode_32("deadbeef").is_err());
        assert!(decode_32(&"00".repeat(33)).is_err());
    }
}
