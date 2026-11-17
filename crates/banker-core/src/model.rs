//! The data the app shows. Every finding is either something that was deleted
//! or something that is damaged; Banker never lists healthy, live content.

use serde::{Deserialize, Serialize};

/// Why a finding is shown.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    /// Removed by the user or an app, but some or all of it is still there.
    Deleted,
    /// Present, but damaged: truncated, zero-filled or structurally broken.
    Corrupt,
}

/// Where the evidence came from. Decides how much of the item survives.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    /// A whole file sitting in a trash or recycle folder.
    TrashFolder,
    /// A photo or video in Photos' "Recently Deleted" album.
    PhotosRecentlyDeleted,
    /// Text recovered from free space inside an app's SQLite database.
    DatabaseFragment,
    /// An older version of a database page, kept in its write-ahead log.
    DatabaseHistory,
    /// A picture rebuilt from free space inside an app's database.
    CarvedFromDatabase,
    /// A file that is present but fails its format's integrity checks.
    DamagedFile,
}

/// What kind of content it is, for filtering and for choosing a preview.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum Media {
    Image,
    Video,
    Audio,
    Document,
    Archive,
    Text,
    Database,
    Other,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// Stable within one report; used to ask for previews and exports.
    pub id: u64,
    pub status: Status,
    pub source: Source,
    pub media: Media,
    /// A short name to show: a file name, or the database a fragment came from.
    pub name: String,
    /// One sentence saying why this is shown.
    pub reason: String,
    /// Where the bytes are on this Mac, when there are bytes to show.
    pub path: Option<String>,
    /// Where the item lived on the device, when that differs from `path`.
    pub original_path: Option<String>,
    /// The app the item belongs to: a bundle id, package name or app name.
    pub app: Option<String>,
    pub size: u64,
    /// Unix seconds. The deletion time when known, otherwise the last change.
    pub time: Option<i64>,
    /// True when `time` was inferred from nearby data rather than recorded.
    pub time_estimated: bool,
    /// Recovered text, for database fragments.
    pub text: Option<String>,
}

impl Finding {
    pub(crate) fn new(status: Status, source: Source, media: Media, name: String) -> Self {
        Finding {
            id: 0,
            status,
            source,
            media,
            name,
            reason: String::new(),
            path: None,
            original_path: None,
            app: None,
            size: 0,
            time: None,
            time_estimated: false,
            text: None,
        }
    }
}

/// The result of scanning one device or folder.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// The folder that was examined (the backup, for phones).
    pub root: String,
    pub findings: Vec<Finding>,
    pub files_checked: u64,
    pub databases_checked: u64,
    /// Plain-language notes about what could not be examined, and why.
    pub notes: Vec<String>,
}

impl Report {
    pub(crate) fn push(&mut self, mut finding: Finding) {
        finding.id = self.findings.len() as u64 + 1;
        self.findings.push(finding);
    }

    pub fn finding(&self, id: u64) -> Option<&Finding> {
        let i = usize::try_from(id).ok()?.checked_sub(1)?;
        self.findings.get(i).filter(|f| f.id == id)
    }

    pub(crate) fn note(&mut self, note: impl Into<String>) {
        let note = note.into();
        if !self.notes.contains(&note) {
            self.notes.push(note);
        }
    }
}

/// Classify content by file name.
pub fn media_for_name(name: &str) -> Media {
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "jpg" | "jpeg" | "png" | "gif" | "heic" | "heif" | "webp" | "bmp" | "tif" | "tiff"
        | "dng" | "cr2" | "cr3" | "nef" | "arw" | "raf" => Media::Image,
        "mov" | "mp4" | "m4v" | "3gp" | "avi" | "mkv" | "webm" => Media::Video,
        "mp3" | "m4a" | "aac" | "wav" | "aiff" | "flac" | "ogg" | "opus" | "amr" | "caf" => {
            Media::Audio
        }
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "pages" | "numbers" | "key"
        | "rtf" | "odt" | "epub" => Media::Document,
        "zip" | "rar" | "7z" | "gz" | "tar" | "apk" | "ipa" | "dmg" => Media::Archive,
        "txt" | "md" | "csv" | "json" | "xml" | "html" | "vcf" | "ics" | "log" => Media::Text,
        "db" | "sqlite" | "sqlite3" | "sqlitedb" | "storedata" => Media::Database,
        _ => Media::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_by_extension_case_insensitively() {
        assert_eq!(media_for_name("IMG_0001.HEIC"), Media::Image);
        assert_eq!(media_for_name("clip.mov"), Media::Video);
        assert_eq!(media_for_name("notes.pdf"), Media::Document);
        assert_eq!(media_for_name("no-extension"), Media::Other);
    }

    #[test]
    fn report_numbers_findings_from_one() {
        let mut report = Report::default();
        let f = Finding::new(
            Status::Deleted,
            Source::TrashFolder,
            Media::Image,
            "a".into(),
        );
        report.push(f.clone());
        report.push(f);
        assert_eq!(report.findings[0].id, 1);
        assert_eq!(report.findings[1].id, 2);
    }
}
