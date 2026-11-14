//! How to show a finding: the picture itself, a player, the document, or its
//! text. Chosen from the file's real contents, not only its name, because
//! files in trash folders and backups are often misnamed or nameless.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use serde::Serialize;

use crate::export::thumbnail;
use crate::integrity::{sniff, Kind};
use crate::model::{Finding, Media};

/// Text files are previewed up to this many bytes.
const TEXT_LIMIT: usize = 64 * 1024;
/// A card shows this many characters of text.
const SNIPPET_CHARS: usize = 400;

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub enum Preview {
    /// A picture the window can show at full size.
    Image {
        path: String,
    },
    Video {
        path: String,
    },
    Audio {
        path: String,
    },
    Pdf {
        path: String,
    },
    /// Readable text: a recovered record, or a text file's contents.
    Text {
        text: String,
        truncated: bool,
    },
    /// A rendered picture of a file the window cannot show itself
    /// (RAW photos, Word documents, unusual video formats).
    Picture {
        data_url: String,
    },
    /// Nothing to show, and why.
    Nothing {
        reason: String,
    },
}

impl Preview {
    /// The file the window needs to read to show this, if any.
    pub fn file(&self) -> Option<&str> {
        match self {
            Preview::Image { path }
            | Preview::Video { path }
            | Preview::Audio { path }
            | Preview::Pdf { path } => Some(path),
            _ => None,
        }
    }
}

/// What a card in the grid shows: a small picture, or a snippet of text.
#[derive(Serialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CardPreview {
    pub image: Option<String>,
    pub text: Option<String>,
}

fn extension(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default()
}

fn head(path: &Path, len: usize) -> Option<Vec<u8>> {
    let mut buf = Vec::with_capacity(len);
    File::open(path)
        .ok()?
        .take(len as u64)
        .read_to_end(&mut buf)
        .ok()?;
    Some(buf)
}

/// Text the person can read: valid UTF-8 (a cut-off last character is
/// fine) with almost no control characters.
fn as_text(bytes: &[u8]) -> Option<String> {
    let text = match std::str::from_utf8(bytes) {
        Ok(t) => t.to_string(),
        Err(e) if e.valid_up_to() + 4 >= bytes.len() => {
            String::from_utf8_lossy(&bytes[..e.valid_up_to()]).into_owned()
        }
        Err(_) => return None,
    };
    let controls = text
        .chars()
        .filter(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
        .count();
    (!text.trim().is_empty() && controls * 100 <= text.chars().count()).then_some(text)
}

const TEXT_EXTENSIONS: &[&str] = &[
    "txt", "md", "csv", "tsv", "json", "xml", "html", "htm", "vcf", "ics", "log", "srt", "vtt",
    "yaml", "yml", "ini", "conf", "eml",
];
/// Formats WebKit on macOS shows or plays itself.
const WEB_IMAGES: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "webp", "heic", "heif", "bmp", "tif", "tiff", "avif",
];
const WEB_VIDEO: &[&str] = &["mp4", "mov", "m4v", "3gp"];
const WEB_AUDIO: &[&str] = &[
    "mp3", "m4a", "aac", "wav", "aiff", "aif", "caf", "flac", "opus", "ogg",
];

/// What `path` holds, from its first bytes, falling back to the name.
fn content_kind(path: &Path, name: &str) -> (Option<Kind>, String) {
    let first = head(path, 64).unwrap_or_default();
    (sniff(&first), extension(name))
}

/// The full preview for the detail panel.
pub fn preview(f: &Finding, cache: &Path) -> Preview {
    if let Some(text) = &f.text {
        return Preview::Text {
            text: text.clone(),
            truncated: false,
        };
    }
    let Some(path) = f.path.as_deref() else {
        return Preview::Nothing {
            reason: "Only its record is left; the file itself is gone.".into(),
        };
    };
    let file = Path::new(path);
    let Ok(meta) = std::fs::metadata(file) else {
        return Preview::Nothing {
            reason: "The file is no longer where it was found.".into(),
        };
    };
    if meta.len() == 0 {
        return Preview::Nothing {
            reason: "The file is empty; there is nothing left to show.".into(),
        };
    }
    let owned = path.to_string();
    let (kind, ext) = content_kind(file, &f.name);
    match kind {
        Some(Kind::Jpeg | Kind::Png | Kind::Gif) => return Preview::Image { path: owned },
        Some(Kind::Pdf) => return Preview::Pdf { path: owned },
        Some(Kind::IsoMedia) if WEB_IMAGES.contains(&ext.as_str()) => {
            return Preview::Image { path: owned }
        }
        Some(Kind::IsoMedia) if f.media == Media::Audio => return Preview::Audio { path: owned },
        Some(Kind::IsoMedia) => return Preview::Video { path: owned },
        _ => {}
    }
    match f.media {
        Media::Image if WEB_IMAGES.contains(&ext.as_str()) => {
            return Preview::Image { path: owned }
        }
        Media::Video if WEB_VIDEO.contains(&ext.as_str()) => return Preview::Video { path: owned },
        Media::Audio if WEB_AUDIO.contains(&ext.as_str()) => return Preview::Audio { path: owned },
        _ => {}
    }
    if TEXT_EXTENSIONS.contains(&ext.as_str()) || matches!(f.media, Media::Text | Media::Other) {
        if let Some(text) = head(file, TEXT_LIMIT).and_then(|b| as_text(&b)) {
            return Preview::Text {
                text,
                truncated: meta.len() as usize > TEXT_LIMIT,
            };
        }
    }
    if let Some(data_url) = thumbnail(f, cache) {
        return Preview::Picture { data_url };
    }
    Preview::Nothing {
        reason: "This kind of file cannot be previewed. Save it to the Mac to open it.".into(),
    }
}

/// The small preview for a card: a thumbnail for pictures, videos and
/// documents, the opening lines for text.
pub fn card_preview(f: &Finding, cache: &Path) -> CardPreview {
    if let Some(text) = &f.text {
        return CardPreview {
            text: Some(text.chars().take(SNIPPET_CHARS).collect()),
            image: None,
        };
    }
    let Some(path) = f.path.as_deref().map(Path::new).filter(|p| p.is_file()) else {
        return CardPreview::default();
    };
    if matches!(f.media, Media::Image | Media::Video | Media::Document) {
        if let Some(image) = thumbnail(f, cache) {
            return CardPreview {
                image: Some(image),
                text: None,
            };
        }
    }
    let ext = extension(&f.name);
    if TEXT_EXTENSIONS.contains(&ext.as_str()) || matches!(f.media, Media::Text | Media::Other) {
        if let Some(text) = head(path, SNIPPET_CHARS * 4).and_then(|b| as_text(&b)) {
            return CardPreview {
                text: Some(text.chars().take(SNIPPET_CHARS).collect()),
                image: None,
            };
        }
    }
    CardPreview::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrity::tests::{temp_dir, tiny_jpeg};
    use crate::model::{Source, Status};

    fn finding(dir: &Path, name: &str, data: &[u8], media: Media) -> Finding {
        let path = dir.join(name.replace('/', "_"));
        std::fs::write(&path, data).unwrap();
        let mut f = Finding::new(Status::Deleted, Source::TrashFolder, media, name.into());
        f.path = Some(path.display().to_string());
        f
    }

    #[test]
    fn chooses_the_right_viewer_from_contents() {
        let dir = temp_dir("preview");
        let cache = dir.join("cache");
        let p = |name: &str, data: &[u8], media| preview(&finding(&dir, name, data, media), &cache);

        assert!(matches!(
            p("a.jpg", &tiny_jpeg(), Media::Image),
            Preview::Image { .. }
        ));
        // Misnamed: a JPEG saved without its extension still shows as a picture.
        assert!(matches!(
            p("1234", &tiny_jpeg(), Media::Other),
            Preview::Image { .. }
        ));
        assert!(matches!(
            p("doc.pdf", b"%PDF-1.4\n%%EOF", Media::Document),
            Preview::Pdf { .. }
        ));

        let mut mp4 = 16u32.to_be_bytes().to_vec();
        mp4.extend_from_slice(b"ftypisom\0\0\0\0");
        assert!(matches!(
            p("clip.mov", &mp4, Media::Video),
            Preview::Video { .. }
        ));
        assert!(matches!(
            p("note.m4a", &mp4, Media::Audio),
            Preview::Audio { .. }
        ));
        assert!(matches!(
            p("voice.opus", b"OggS\0\x02", Media::Audio),
            Preview::Audio { .. }
        ));

        match p("list.txt", "milk\neggs ☕\n".as_bytes(), Media::Text) {
            Preview::Text { text, truncated } => {
                assert_eq!(text, "milk\neggs ☕\n");
                assert!(!truncated);
            }
            other => panic!("{other:?}"),
        }
        match p(
            "contact.vcf",
            b"BEGIN:VCARD\nFN:Alex\nEND:VCARD",
            Media::Text,
        ) {
            Preview::Text { text, .. } => assert!(text.contains("Alex")),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            p("blob.bin", &[0, 1, 2, 3, 0, 0, 9], Media::Other),
            Preview::Nothing { .. }
        ));
        assert!(matches!(
            p("empty.jpg", b"", Media::Image),
            Preview::Nothing { .. }
        ));
    }

    #[test]
    fn serialises_the_shape_the_ui_reads() {
        let picture = Preview::Picture {
            data_url: "data:x".into(),
        };
        assert_eq!(
            serde_json::to_string(&picture).unwrap(),
            r#"{"kind":"picture","dataUrl":"data:x"}"#
        );
        let text = Preview::Text {
            text: "hi".into(),
            truncated: true,
        };
        assert_eq!(
            serde_json::to_string(&text).unwrap(),
            r#"{"kind":"text","text":"hi","truncated":true}"#
        );
    }

    #[test]
    fn records_and_missing_files() {
        let dir = temp_dir("preview-records");
        let mut rec = Finding::new(
            Status::Deleted,
            Source::DatabaseFragment,
            Media::Text,
            "chat.db".into(),
        );
        rec.text = Some("see you at 8".into());
        assert_eq!(
            preview(&rec, &dir),
            Preview::Text {
                text: "see you at 8".into(),
                truncated: false
            }
        );
        let gone = Finding::new(
            Status::Deleted,
            Source::PhotosRecentlyDeleted,
            Media::Image,
            "IMG.HEIC".into(),
        );
        match preview(&gone, &dir) {
            Preview::Nothing { reason } => assert!(reason.contains("record")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn long_text_is_truncated_and_cards_get_a_snippet() {
        let dir = temp_dir("preview-text");
        let long = "word ".repeat(20_000);
        let f = finding(&dir, "big.log", long.as_bytes(), Media::Text);
        match preview(&f, &dir) {
            Preview::Text { text, truncated } => {
                assert!(truncated);
                assert!(text.len() <= TEXT_LIMIT);
            }
            other => panic!("{other:?}"),
        }
        let card = card_preview(&f, &dir);
        assert_eq!(card.text.unwrap().chars().count(), SNIPPET_CHARS);
        assert!(card.image.is_none());

        let pic = finding(&dir, "a.jpg", &tiny_jpeg(), Media::Image);
        assert!(card_preview(&pic, &dir.join("c"))
            .image
            .unwrap()
            .starts_with("data:image/jpeg"));
    }
}
