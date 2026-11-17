//! Small helpers that would otherwise each pull in a dependency.

/// CRC-32 (IEEE), as used by PNG chunks.
pub fn crc32(parts: &[&[u8]]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for part in parts {
        for &byte in *part {
            crc ^= byte as u32;
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
            }
        }
    }
    !crc
}

/// Standard base64 with padding.
pub fn base64(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

pub fn be_u16(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(data.get(at..at + 2)?.try_into().ok()?))
}

pub fn be_u32(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

pub fn be_u64(data: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(data.get(at..at + 8)?.try_into().ok()?))
}

pub fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || from >= haystack.len() {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

pub fn rfind(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack.windows(needle.len()).rposition(|w| w == needle)
}

/// Runs of readable text in arbitrary bytes, at least `min_chars` long.
///
/// Accepts printable ASCII and valid multi-byte UTF-8, so recovered messages
/// in any language survive. A run must contain a letter or digit, which
/// filters out the punctuation noise that binary data is full of.
pub fn text_runs(data: &[u8], min_chars: usize) -> Vec<(usize, String)> {
    let mut runs = Vec::new();
    let mut start = 0;
    let mut chars = 0;
    let mut i = 0;
    let flush = |runs: &mut Vec<(usize, String)>, start: usize, end: usize, chars: usize| {
        if chars >= min_chars {
            if let Ok(s) = std::str::from_utf8(&data[start..end]) {
                let trimmed = s.trim();
                if trimmed.chars().count() >= min_chars
                    && trimmed.chars().any(|c| c.is_alphanumeric())
                {
                    runs.push((start, trimmed.to_string()));
                }
            }
        }
    };
    while i < data.len() {
        let b = data[i];
        let len = if (0x20..0x7F).contains(&b) || b == b'\n' || b == b'\t' {
            1
        } else if b >= 0xC2 {
            let want = if b < 0xE0 {
                2
            } else if b < 0xF0 {
                3
            } else if b < 0xF5 {
                4
            } else {
                0
            };
            if want > 0 && i + want <= data.len() && std::str::from_utf8(&data[i..i + want]).is_ok()
            {
                want
            } else {
                0
            }
        } else {
            0
        };
        if len == 0 {
            flush(&mut runs, start, i, chars);
            i += 1;
            start = i;
            chars = 0;
        } else {
            i += len;
            chars += 1;
        }
    }
    flush(&mut runs, start, data.len(), chars);
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_matches_known_value() {
        assert_eq!(crc32(&[b"IEND"]), 0xAE42_6082);
        assert_eq!(crc32(&[b"IE", b"ND"]), 0xAE42_6082);
    }

    #[test]
    fn base64_pads() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(base64(b""), "");
    }

    #[test]
    fn finds_text_runs_in_binary_including_unicode() {
        let mut data = vec![0u8, 1, 2];
        data.extend_from_slice("See you at 8 ☕".as_bytes());
        data.extend_from_slice(&[0, 0xFF, b'a', b'b', 0]);
        data.extend_from_slice(b"!!!!!!");
        let runs = text_runs(&data, 4);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].0, 3);
        assert_eq!(runs[0].1, "See you at 8 ☕");
    }

    #[test]
    fn find_and_rfind() {
        assert_eq!(find(b"abcabc", b"bc", 0), Some(1));
        assert_eq!(find(b"abcabc", b"bc", 2), Some(4));
        assert_eq!(rfind(b"abcabc", b"ab"), Some(3));
        assert_eq!(find(b"ab", b"abc", 0), None);
    }
}
