//! Rebuild pictures from raw bytes that no longer belong to any file or row.
//!
//! Only ever run over space that is known to be free (a database's freelist,
//! freeblocks and unallocated gaps), so everything found was deleted.

use crate::integrity::{jpeg_end, png_end, Kind};
use crate::util::find;

/// Pictures smaller than this are almost always icons or noise.
const MIN_SIZE: usize = 256;

/// Complete pictures found in `data`, as (kind, start, end).
pub fn carve_pictures(data: &[u8]) -> Vec<(Kind, usize, usize)> {
    let mut found = Vec::new();
    let mut pos = 0;
    while pos + 8 <= data.len() {
        let rest = &data[pos..];
        let hit = if rest.starts_with(&[0xFF, 0xD8, 0xFF]) {
            jpeg_end(data, pos).ok().map(|end| (Kind::Jpeg, end))
        } else if rest.starts_with(b"\x89PNG\r\n\x1a\n") {
            png_end(data, pos).ok().map(|end| (Kind::Png, end))
        } else if rest.starts_with(b"GIF89a") || rest.starts_with(b"GIF87a") {
            find(data, b"\x00\x3B", pos + 13).map(|e| (Kind::Gif, e + 2))
        } else {
            None
        };
        match hit {
            Some((kind, end)) if end - pos >= MIN_SIZE => {
                found.push((kind, pos, end));
                pos = end;
            }
            _ => pos += 1,
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrity::tests::{tiny_jpeg, tiny_png};

    fn padded(mut picture: Vec<u8>) -> Vec<u8> {
        // Pad an APP segment so the picture clears MIN_SIZE.
        let pad = vec![0x41u8; 400];
        let mut seg = vec![0xFF, 0xEE];
        seg.extend_from_slice(&((pad.len() + 2) as u16).to_be_bytes());
        seg.extend_from_slice(&pad);
        picture.splice(2..2, seg);
        picture
    }

    #[test]
    fn carves_pictures_out_of_noise() {
        let jpeg = padded(tiny_jpeg());
        let mut data = vec![7u8; 100];
        data.extend_from_slice(&jpeg);
        data.extend_from_slice(&[0u8; 50]);
        data.extend_from_slice(&tiny_png()); // too small: ignored
        let found = carve_pictures(&data);
        assert_eq!(found, vec![(Kind::Jpeg, 100, 100 + jpeg.len())]);
    }

    #[test]
    fn ignores_truncated_pictures() {
        let jpeg = padded(tiny_jpeg());
        assert!(carve_pictures(&jpeg[..jpeg.len() - 4]).is_empty());
    }
}
