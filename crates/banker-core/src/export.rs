//! Showing and saving findings: thumbnails, and copying recovered items to
//! a folder organised by app and day.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::integrity::{sniff, Kind};
use crate::model::{Finding, Media};
use crate::paths::app_display_name;
use crate::timeline::civil_from_days;
use crate::util::base64;

const INLINE_LIMIT: u64 = 4 * 1024 * 1024;

fn data_url(bytes: &[u8]) -> Option<String> {
    let mime = match sniff(bytes) {
        Some(Kind::Jpeg) => "image/jpeg",
        Some(Kind::Png) => "image/png",
        Some(Kind::Gif) => "image/gif",
        _ if bytes.get(8..12) == Some(b"WEBP") => "image/webp",
        _ => return None,
    };
    Some(format!("data:{mime};base64,{}", base64(bytes)))
}

/// A small picture of a finding, as a data URL, or None if it has none.
///
/// On a Mac, `sips` (pictures, including HEIC and RAW) and `qlmanage`
/// (videos, PDFs) make a small JPEG or PNG, cached in `cache`. Elsewhere,
/// small pictures are shown as they are.
pub fn thumbnail(f: &Finding, cache: &Path) -> Option<String> {
    let path = PathBuf::from(f.path.as_ref()?);
    if !matches!(f.media, Media::Image | Media::Video | Media::Document) || !path.is_file() {
        return None;
    }
    let cached = cache.join(format!("{}.jpg", f.id));
    if let Ok(bytes) = fs::read(&cached) {
        return data_url(&bytes);
    }
    let _ = fs::create_dir_all(cache);
    if f.media == Media::Image && Path::new("/usr/bin/sips").is_file() {
        let made = Command::new("/usr/bin/sips")
            .args(["-s", "format", "jpeg", "-Z", "360"])
            .arg(&path)
            .arg("--out")
            .arg(&cached)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if made {
            return data_url(&fs::read(&cached).ok()?);
        }
    }
    if Path::new("/usr/bin/qlmanage").is_file() {
        let dir = cache.join(format!("ql-{}", f.id));
        let _ = fs::create_dir_all(&dir);
        let ok = Command::new("/usr/bin/qlmanage")
            .args(["-t", "-s", "360", "-o"])
            .arg(&dir)
            .arg(&path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if ok {
            let png = fs::read_dir(&dir)
                .ok()?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .find(|p| p.extension().is_some_and(|e| e == "png"))?;
            let bytes = fs::read(&png).ok()?;
            let _ = fs::write(&cached, &bytes);
            let _ = fs::remove_dir_all(&dir);
            return data_url(&bytes);
        }
    }
    if fs::metadata(&path).ok()?.len() <= INLINE_LIMIT {
        return data_url(&fs::read(&path).ok()?);
    }
    None
}

fn day(t: Option<i64>, offset: i64) -> String {
    match t {
        Some(t) => {
            let (y, m, d) = civil_from_days((t + offset).div_euclid(86_400));
            format!("{y}-{m:02}-{d:02}")
        }
        None => "Undated".into(),
    }
}

fn clean(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if matches!(c, '/' | ':' | '\\' | '\0') {
                '-'
            } else {
                c
            }
        })
        .collect();
    let s = s.trim().trim_start_matches('.').to_string();
    if s.is_empty() {
        "item".into()
    } else {
        s
    }
}

fn unique(path: PathBuf) -> PathBuf {
    if !path.exists() {
        return path;
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
    (2..)
        .map(|n| parent.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .unwrap_or(path)
}

#[derive(Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Exported {
    pub folder: String,
    pub files: usize,
    pub records: usize,
    pub failed: usize,
}

/// Save findings into `dest/<App>/<YYYY-MM-DD>/`. Files are copied; text
/// recovered from databases goes into one readable file per app and day.
pub fn export(
    findings: &[&Finding],
    dest: &Path,
    tz_offset_minutes: i32,
) -> std::io::Result<Exported> {
    let offset = tz_offset_minutes as i64 * 60;
    fs::create_dir_all(dest)?;
    let mut out = Exported {
        folder: dest.display().to_string(),
        ..Exported::default()
    };
    let mut notes: HashMap<PathBuf, String> = HashMap::new();
    for f in findings {
        let app = f
            .app
            .as_deref()
            .map(app_display_name)
            .unwrap_or_else(|| "Other".into());
        let folder = dest.join(clean(&app)).join(day(f.time, offset));
        if let Some(text) = &f.text {
            let entry = notes
                .entry(folder.join("Deleted messages and records.txt"))
                .or_default();
            let when = f.time.map(|t| {
                let s = (t + offset).rem_euclid(86_400);
                format!(
                    "{:02}:{:02}{} ",
                    s / 3600,
                    s % 3600 / 60,
                    if f.time_estimated { " (approx.)" } else { "" }
                )
            });
            entry.push_str(&format!(
                "{}{}\n{}\n\n",
                when.unwrap_or_default(),
                f.reason,
                text
            ));
            out.records += 1;
            continue;
        }
        let Some(src) = f.path.as_ref().map(PathBuf::from).filter(|p| p.is_file()) else {
            continue;
        };
        let copied = fs::create_dir_all(&folder)
            .and_then(|_| fs::copy(&src, unique(folder.join(clean(&f.name)))));
        match copied {
            Ok(_) => out.files += 1,
            Err(_) => out.failed += 1,
        }
    }
    for (path, text) in notes {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?
            .write_all(text.as_bytes())?;
    }
    Ok(out)
}

/// Save every finding that matches `q` into a new, dated folder in
/// `~/Banker Backups`.
pub fn export_matching(
    report: &crate::model::Report,
    q: &crate::timeline::Query,
    home: &Path,
    now: i64,
) -> std::io::Result<Exported> {
    let all = crate::timeline::Query {
        limit: usize::MAX,
        ..q.clone()
    };
    let result = crate::timeline::query(&report.findings, &all);
    let picked: Vec<&Finding> = result.items.iter().collect();
    let dest =
        crate::devices::backup_folder(home, "Recovered", now + q.tz_offset_minutes as i64 * 60);
    export(&picked, &dest, q.tz_offset_minutes)
}

/// Show a file in Finder.
pub fn reveal(path: &str) -> Result<(), String> {
    Command::new("/usr/bin/open")
        .args(["-R", path])
        .status()
        .map_err(|e| e.to_string())
        .and_then(|s| {
            s.success()
                .then_some(())
                .ok_or_else(|| "Finder could not show it.".into())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrity::tests::{temp_dir, tiny_jpeg};
    use crate::model::{Source, Status};

    #[test]
    fn exports_by_app_and_day() {
        let dir = temp_dir("export");
        let src = dir.join("a.jpg");
        fs::write(&src, tiny_jpeg()).unwrap();
        let mut photo = Finding::new(
            Status::Deleted,
            Source::TrashFolder,
            Media::Image,
            "a.jpg".into(),
        );
        photo.path = Some(src.display().to_string());
        photo.app = Some("com.whatsapp".into());
        photo.time = Some(crate::timeline::days_from_civil(2025, 6, 1) * 86_400 + 3600);
        let twin = photo.clone();
        let mut text = Finding::new(
            Status::Deleted,
            Source::DatabaseFragment,
            Media::Text,
            "chat.db".into(),
        );
        text.text = Some("hello again".into());
        text.reason = "Deleted from chat.db".into();
        text.app = Some("com.whatsapp".into());

        let out = export(&[&photo, &twin, &text], &dir.join("out"), 0).unwrap();
        assert_eq!((out.files, out.records, out.failed), (2, 1, 0));
        let day = dir.join("out/WhatsApp/2025-06-01");
        assert!(day.join("a.jpg").is_file());
        assert!(day.join("a (2).jpg").is_file());
        let notes =
            fs::read_to_string(dir.join("out/WhatsApp/Undated/Deleted messages and records.txt"))
                .unwrap();
        assert!(notes.contains("hello again"));
    }

    #[test]
    fn small_pictures_inline() {
        let dir = temp_dir("thumb");
        let src = dir.join("a.jpg");
        fs::write(&src, tiny_jpeg()).unwrap();
        let mut f = Finding::new(
            Status::Deleted,
            Source::TrashFolder,
            Media::Image,
            "a.jpg".into(),
        );
        f.path = Some(src.display().to_string());
        let url = thumbnail(&f, &dir.join("cache")).unwrap();
        assert!(url.starts_with("data:image/jpeg;base64,"));
    }
}
