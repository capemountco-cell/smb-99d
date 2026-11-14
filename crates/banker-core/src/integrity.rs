//! Is a file damaged? Each check walks the format's own structure, so a file
//! is only called corrupt when its bytes prove it: a missing end marker, a
//! failed checksum, a box that runs past the end of the file.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::model::{media_for_name, Media};
use crate::util::{be_u16, be_u32, be_u64, crc32, find, rfind};

/// Formats recognised by their first bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Jpeg,
    Png,
    Gif,
    Pdf,
    Zip,
    IsoMedia,
    Riff,
    Sqlite,
}

impl Kind {
    pub fn extension(self) -> &'static str {
        match self {
            Kind::Jpeg => "jpg",
            Kind::Png => "png",
            Kind::Gif => "gif",
            Kind::Pdf => "pdf",
            Kind::Zip => "zip",
            Kind::IsoMedia => "mp4",
            Kind::Riff => "wav",
            Kind::Sqlite => "sqlite",
        }
    }
}

pub fn sniff(head: &[u8]) -> Option<Kind> {
    if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(Kind::Jpeg)
    } else if head.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(Kind::Png)
    } else if head.starts_with(b"GIF87a") || head.starts_with(b"GIF89a") {
        Some(Kind::Gif)
    } else if head.starts_with(b"%PDF-") {
        Some(Kind::Pdf)
    } else if head.starts_with(b"PK\x03\x04") {
        Some(Kind::Zip)
    } else if head.starts_with(b"SQLite format 3\0") {
        Some(Kind::Sqlite)
    } else if head.starts_with(b"RIFF") && head.len() >= 12 {
        Some(Kind::Riff)
    } else if head.len() >= 8
        && matches!(
            &head[4..8],
            b"ftyp" | b"moov" | b"mdat" | b"wide" | b"free" | b"skip" | b"pnot"
        )
    {
        Some(Kind::IsoMedia)
    } else {
        None
    }
}

/// The kind a file's extension promises, for extensions we can verify.
fn expected_kind(name: &str) -> Option<Kind> {
    let ext = name.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match ext.as_str() {
        "jpg" | "jpeg" => Kind::Jpeg,
        "png" => Kind::Png,
        "gif" => Kind::Gif,
        "pdf" => Kind::Pdf,
        "zip" | "docx" | "xlsx" | "pptx" | "epub" | "apk" | "ipa" | "jar" => Kind::Zip,
        "mp4" | "mov" | "m4v" | "m4a" | "3gp" | "heic" | "heif" => Kind::IsoMedia,
        "wav" | "avi" | "webp" => Kind::Riff,
        _ => return None,
    })
}

/// Where a JPEG that starts at `start` ends, following its marker segments
/// and entropy-coded data rather than searching for the first FF D9 (which an
/// embedded thumbnail also contains).
pub fn jpeg_end(data: &[u8], start: usize) -> Result<usize, &'static str> {
    const TRUNCATED: &str = "The picture stops partway through; the rest of the file is missing";
    if data.get(start..start + 2) != Some(&[0xFF, 0xD8]) {
        return Err("The picture's header is damaged");
    }
    let mut pos = start + 2;
    loop {
        if pos >= data.len() {
            return Err(TRUNCATED);
        }
        if data[pos] != 0xFF {
            return Err("The picture's internal structure is damaged");
        }
        while pos < data.len() && data[pos] == 0xFF {
            pos += 1;
        }
        let Some(&marker) = data.get(pos) else {
            return Err(TRUNCATED);
        };
        pos += 1;
        match marker {
            0xD9 => return Ok(pos),
            0x01 | 0xD0..=0xD7 => continue,
            0x00 => return Err("The picture's internal structure is damaged"),
            _ => {}
        }
        let len = be_u16(data, pos).ok_or(TRUNCATED)? as usize;
        if len < 2 {
            return Err("The picture's internal structure is damaged");
        }
        pos += len;
        if marker == 0xDA {
            // Entropy-coded data: FF is always followed by 00, a restart
            // marker or a real marker.
            loop {
                if pos + 1 >= data.len() {
                    return Err(TRUNCATED);
                }
                if data[pos] == 0xFF {
                    match data[pos + 1] {
                        0x00 | 0xD0..=0xD7 => pos += 2,
                        0xFF => pos += 1,
                        _ => break,
                    }
                } else {
                    pos += 1;
                }
            }
        }
    }
}

/// Where a PNG that starts at `start` ends, checking every chunk's CRC.
pub fn png_end(data: &[u8], start: usize) -> Result<usize, &'static str> {
    const TRUNCATED: &str = "The picture stops partway through; the rest of the file is missing";
    let mut pos = start + 8;
    loop {
        let len = be_u32(data, pos).ok_or(TRUNCATED)? as usize;
        let kind = data.get(pos + 4..pos + 8).ok_or(TRUNCATED)?;
        let body_end = pos.checked_add(8 + len).ok_or(TRUNCATED)?;
        let body = data.get(pos + 8..body_end).ok_or(TRUNCATED)?;
        let stored = be_u32(data, body_end).ok_or(TRUNCATED)?;
        if crc32(&[kind, body]) != stored {
            return Err("Part of the picture's data is corrupted (a checksum fails)");
        }
        let next = body_end + 4;
        if kind == b"IEND" {
            return Ok(next);
        }
        pos = next;
    }
}

const SAMPLE: usize = 64 * 1024;
/// Formats checked by reading the whole file are skipped above this size.
const WHOLE_FILE_LIMIT: u64 = 256 * 1024 * 1024;

fn read_at(file: &mut File, offset: u64, len: usize) -> std::io::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(offset))?;
    let mut buf = Vec::with_capacity(len);
    file.take(len as u64).read_to_end(&mut buf)?;
    Ok(buf)
}

/// Check a file. Returns why it is damaged, or None if it looks intact or
/// cannot be judged. `name` is the file's name on the device, which can
/// differ from the name on disk (iPhone backups store files by hash).
pub fn check(path: &Path, name: &str) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    let media = media_for_name(name);
    let verifiable = matches!(
        media,
        Media::Image | Media::Video | Media::Audio | Media::Document | Media::Archive
    );
    if size == 0 {
        return verifiable.then(|| "The file is empty (0 bytes); its contents are gone".into());
    }
    let head = read_at(&mut file, 0, SAMPLE).ok()?;

    if verifiable && size >= 4096 && head.iter().all(|&b| b == 0) {
        let middle = read_at(&mut file, size / 2, SAMPLE).ok()?;
        let tail = read_at(&mut file, size.saturating_sub(SAMPLE as u64), SAMPLE).ok()?;
        if middle.iter().all(|&b| b == 0) && tail.iter().all(|&b| b == 0) {
            return Some("The file is filled with zeros; its contents were lost".into());
        }
    }

    let Some(kind) = sniff(&head) else {
        return expected_kind(name).map(|expected| {
            format!(
                "The start of the file is damaged: it is named .{} but its contents do not match",
                name.rsplit_once('.')
                    .map(|(_, e)| e)
                    .unwrap_or(expected.extension())
            )
        });
    };

    let whole = |file: &mut File| -> Option<Vec<u8>> {
        (size <= WHOLE_FILE_LIMIT)
            .then(|| read_at(file, 0, size as usize).ok())
            .flatten()
    };

    match kind {
        Kind::Jpeg => jpeg_end(&whole(&mut file)?, 0).err().map(String::from),
        Kind::Png => png_end(&whole(&mut file)?, 0).err().map(String::from),
        Kind::Gif => {
            let tail = read_at(&mut file, size - 1, 1).ok()?;
            (tail != [0x3B])
                .then(|| "The animation stops partway through; the end is missing".into())
        }
        Kind::Pdf => {
            let tail = read_at(&mut file, size.saturating_sub(1024), 1024).ok()?;
            rfind(&tail, b"%%EOF")
                .is_none()
                .then(|| "The document stops partway through; the end is missing".into())
        }
        Kind::Zip => {
            let span = 65_557u64.min(size);
            let tail = read_at(&mut file, size - span, span as usize).ok()?;
            rfind(&tail, b"PK\x05\x06").is_none().then(|| {
                "The archive stops partway through; its table of contents is missing".into()
            })
        }
        Kind::Riff => {
            let declared = u32::from_le_bytes(head[4..8].try_into().ok()?) as u64 + 8;
            (declared > size).then(|| {
                format!(
                    "The file stops partway through: {} of {} bytes are present",
                    size, declared
                )
            })
        }
        Kind::IsoMedia => check_iso_media(&mut file, size),
        Kind::Sqlite => {
            let page = match be_u16(&head, 16)? {
                1 => 65_536,
                n => n as u64,
            };
            (page >= 512 && size % page != 0)
                .then(|| "The database stops partway through a page; it was cut short".into())
        }
    }
}

/// Walk the top-level boxes of an MP4, MOV or HEIC file.
fn check_iso_media(file: &mut File, size: u64) -> Option<String> {
    let mut pos = 0u64;
    let mut brand = [0u8; 4];
    let mut seen_moov = false;
    let mut seen_meta = false;
    let mut boxes = 0;
    while pos < size {
        let header = read_at(file, pos, 16).ok()?;
        if header.len() < 8 {
            return Some("The file stops partway through; the end is missing".into());
        }
        let mut len = be_u32(&header, 0)? as u64;
        let kind: [u8; 4] = header[4..8].try_into().ok()?;
        if !kind.iter().all(|b| b.is_ascii_graphic() || *b == b' ') {
            return Some("The video's internal structure is damaged".into());
        }
        if len == 1 {
            len = be_u64(&header, 8)?;
        } else if len == 0 {
            len = size - pos;
        }
        if len < 8 {
            return Some("The video's internal structure is damaged".into());
        }
        match &kind {
            b"ftyp" => brand.copy_from_slice(header.get(8..12)?),
            b"moov" => seen_moov = true,
            b"meta" => seen_meta = true,
            _ => {}
        }
        if pos + len > size {
            return Some(format!(
                "The file stops partway through: about {}% is present",
                (size * 100 / (pos + len)).min(99)
            ));
        }
        pos += len;
        boxes += 1;
        if boxes > 100_000 {
            return None;
        }
    }
    let still_image = matches!(&brand, b"heic" | b"heix" | b"mif1" | b"msf1" | b"avif");
    if still_image {
        (!seen_meta).then(|| "The picture's index is missing, so it cannot be opened".into())
    } else {
        (!seen_moov).then(|| "The video's index (moov) is missing, so it will not play".into())
    }
}

/// True if `data` holds a complete, intact file of `kind` from offset 0.
pub fn intact_in_memory(data: &[u8], kind: Kind) -> bool {
    match kind {
        Kind::Jpeg => jpeg_end(data, 0).is_ok(),
        Kind::Png => png_end(data, 0).is_ok(),
        Kind::Gif => data.last() == Some(&0x3B),
        Kind::Pdf => find(data, b"%%EOF", 0).is_some(),
        _ => true,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A minimal but structurally complete JPEG: SOI, APP0, SOS, data, EOI.
    pub fn tiny_jpeg() -> Vec<u8> {
        let mut j = vec![0xFF, 0xD8];
        j.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x06, b'J', b'F', b'I', b'F']);
        // A thumbnail-like EOI inside an APP segment must not end the walk.
        j.extend_from_slice(&[0xFF, 0xE1, 0x00, 0x04, 0xFF, 0xD9]);
        j.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x02]);
        j.extend_from_slice(&[0x12, 0xFF, 0x00, 0x34, 0xFF, 0xD0, 0x56]);
        j.extend_from_slice(&[0xFF, 0xD9]);
        j
    }

    pub fn tiny_png() -> Vec<u8> {
        let mut p = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut chunk = |kind: &[u8], body: &[u8]| {
            p.extend_from_slice(&(body.len() as u32).to_be_bytes());
            p.extend_from_slice(kind);
            p.extend_from_slice(body);
            p.extend_from_slice(&crc32(&[kind, body]).to_be_bytes());
        };
        chunk(b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 0, 0, 0, 0]);
        chunk(b"IEND", &[]);
        p
    }

    fn write(dir: &Path, name: &str, data: &[u8]) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, data).unwrap();
        path
    }

    pub fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "banker-test-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn jpeg_walk_finds_the_real_end() {
        let j = tiny_jpeg();
        assert_eq!(jpeg_end(&j, 0), Ok(j.len()));
        assert!(jpeg_end(&j[..j.len() - 2], 0).is_err());
    }

    #[test]
    fn png_walk_checks_crc() {
        let p = tiny_png();
        assert_eq!(png_end(&p, 0), Ok(p.len()));
        let mut bad = p.clone();
        bad[20] ^= 1;
        assert!(png_end(&bad, 0).unwrap_err().contains("checksum"));
    }

    #[test]
    fn intact_files_pass() {
        let dir = temp_dir("intact");
        assert_eq!(check(&write(&dir, "a.jpg", &tiny_jpeg()), "a.jpg"), None);
        assert_eq!(check(&write(&dir, "a.png", &tiny_png()), "a.png"), None);
        assert_eq!(check(&write(&dir, "empty.txt", b""), "empty.txt"), None);
        assert_eq!(check(&write(&dir, "x.bin", &[1, 2, 3]), "x.bin"), None);
    }

    #[test]
    fn damaged_files_are_named() {
        let dir = temp_dir("damaged");
        let j = tiny_jpeg();
        let cut = write(&dir, "cut.jpg", &j[..j.len() - 3]);
        assert!(check(&cut, "cut.jpg").unwrap().contains("stops partway"));

        let zeros = write(&dir, "z.mov", &vec![0u8; 10_000]);
        assert!(check(&zeros, "z.mov").unwrap().contains("zeros"));

        let wrong = write(&dir, "w.jpg", b"this is not a picture at all");
        assert!(check(&wrong, "w.jpg").unwrap().contains("do not match"));

        let empty = write(&dir, "e.jpg", b"");
        assert!(check(&empty, "e.jpg").unwrap().contains("empty"));

        let pdf = write(&dir, "d.pdf", b"%PDF-1.4\n1 0 obj\n");
        assert!(check(&pdf, "d.pdf").unwrap().contains("stops partway"));
    }

    #[test]
    fn mp4_box_walk() {
        let dir = temp_dir("mp4");
        let mut good = Vec::new();
        good.extend_from_slice(&16u32.to_be_bytes());
        good.extend_from_slice(b"ftypisom\0\0\0\0");
        good.extend_from_slice(&8u32.to_be_bytes());
        good.extend_from_slice(b"moov");
        good.extend_from_slice(&12u32.to_be_bytes());
        good.extend_from_slice(b"mdat\x01\x02\x03\x04");
        assert_eq!(check(&write(&dir, "g.mp4", &good), "g.mp4"), None);

        let cut = write(&dir, "c.mp4", &good[..good.len() - 2]);
        assert!(check(&cut, "c.mp4").unwrap().contains("stops partway"));

        let mut no_index = good[..16].to_vec();
        no_index.extend_from_slice(&good[24..]);
        let ni = write(&dir, "n.mp4", &no_index);
        assert!(check(&ni, "n.mp4").unwrap().contains("moov"));
    }
}
