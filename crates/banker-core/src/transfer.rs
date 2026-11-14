//! Moving a clone's files onto another phone.
//!
//! The organised folders of a clone (Photos & Videos, Apps, Downloads, ...)
//! are listed as groups the person can pick from, whole or file by file.
//!
//! * Onto an Android phone, the picked files go where Android expects them
//!   (photos and videos into the Gallery as their own album), staged as APFS
//!   clones in one tree and sent with a single `adb push`.
//! * Onto an iPhone there is no supported way to add files to its apps, so
//!   "transfer everything" from an iPhone clone is a full restore
//!   ([`crate::clone::restore`]), as Apple does when setting up a new phone.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::devices::{find_tool, run_with, Device, DeviceKind};
use crate::model::{media_for_name, Media};
use crate::scan::Progress;

/// The organised folders of a clone, in the order they are shown.
const GROUP_ROOTS: &[&str] = &[
    "Photos & Videos",
    "Apps",
    "Files on iPhone",
    "Downloads",
    "Documents",
    "Audio",
];

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    /// Path inside the clone, e.g. `Photos & Videos` or `Apps/WhatsApp`.
    pub id: String,
    pub label: String,
    /// What it mostly holds, for its icon.
    pub media: Media,
    pub files: u64,
    pub bytes: u64,
    /// Up to six files to preview on the card.
    pub sample: Vec<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    /// Path inside the clone.
    pub path: String,
    pub name: String,
    pub media: Media,
    pub size: u64,
    pub time: Option<i64>,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    /// Whole groups, by id.
    pub groups: Vec<String>,
    /// Single files, by path inside the clone.
    pub files: Vec<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Transferred {
    pub files: u64,
    pub bytes: u64,
    pub notes: Vec<String>,
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let path = e.path();
        match e.file_type() {
            Ok(t) if t.is_dir() => walk(&path, out),
            Ok(t) if t.is_file() && !e.file_name().to_string_lossy().starts_with('.') => {
                out.push(path)
            }
            _ => {}
        }
    }
}

fn rel(clone: &Path, path: &Path) -> String {
    path.strip_prefix(clone)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// The groups a person can pick from: Photos & Videos, each app, and the
/// other folders, with their sizes.
pub fn groups(clone: &Path) -> Vec<Group> {
    let mut out = Vec::new();
    for root in GROUP_ROOTS {
        let base = clone.join(root);
        if !base.is_dir() {
            continue;
        }
        // Apps are offered one by one; the others as a whole.
        let parts: Vec<(String, PathBuf)> = if *root == "Apps" {
            let mut apps: Vec<_> = fs::read_dir(&base)
                .map(|r| {
                    r.filter_map(Result::ok)
                        .filter(|e| e.path().is_dir())
                        .collect()
                })
                .unwrap_or_default();
            apps.sort_by_key(|e| e.file_name());
            apps.into_iter()
                .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
                .collect()
        } else {
            vec![(root.to_string(), base)]
        };
        for (label, dir) in parts {
            let mut files = Vec::new();
            walk(&dir, &mut files);
            if files.is_empty() {
                continue;
            }
            let mut counts = std::collections::HashMap::new();
            let mut bytes = 0;
            for f in &files {
                bytes += fs::metadata(f).map_or(0, |m| m.len());
                *counts
                    .entry(media_for_name(&f.to_string_lossy()))
                    .or_insert(0u64) += 1;
            }
            let media = counts
                .into_iter()
                .max_by_key(|(_, n)| *n)
                .map_or(Media::Other, |(m, _)| m);
            let sample = files
                .iter()
                .filter(|f| {
                    matches!(
                        media_for_name(&f.to_string_lossy()),
                        Media::Image | Media::Video
                    )
                })
                .rev()
                .take(6)
                .map(|f| rel(clone, f))
                .collect();
            out.push(Group {
                id: rel(clone, &dir),
                label,
                media,
                files: files.len() as u64,
                bytes,
                sample,
            });
        }
    }
    out
}

/// The files of one group, newest first, a page at a time.
pub fn items(
    clone: &Path,
    group: &str,
    offset: usize,
    limit: usize,
) -> Result<(Vec<Item>, usize), String> {
    let dir = inside(clone, group)?;
    let mut files = Vec::new();
    walk(&dir, &mut files);
    let mut items: Vec<Item> = files
        .iter()
        .map(|f| {
            let meta = fs::metadata(f).ok();
            let name = f
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            Item {
                path: rel(clone, f),
                media: media_for_name(&name),
                name,
                size: meta.as_ref().map_or(0, |m| m.len()),
                time: meta
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .and_then(|d| i64::try_from(d.as_secs()).ok()),
            }
        })
        .collect();
    items.sort_by(|a, b| b.time.cmp(&a.time).then(a.path.cmp(&b.path)));
    let total = items.len();
    Ok((items.into_iter().skip(offset).take(limit).collect(), total))
}

/// A path inside the clone's organised folders, refusing anything that
/// would reach outside them.
fn inside(clone: &Path, relative: &str) -> Result<PathBuf, String> {
    let bad = relative.split('/').any(|p| p == ".." || p.is_empty())
        || relative.starts_with('/')
        || relative.contains('\\');
    let top = relative.split('/').next().unwrap_or("");
    if bad || !GROUP_ROOTS.contains(&top) {
        return Err(format!("\"{relative}\" is not part of this clone."));
    }
    Ok(clone.join(relative))
}

/// A small picture of one file in a clone, for the picker.
pub fn thumbnail(clone: &Path, relative: &str, cache: &Path) -> Option<String> {
    use std::hash::{Hash, Hasher};
    let path = inside(clone, relative).ok()?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h);
    let name = path.file_name()?.to_string_lossy().into_owned();
    let mut f = crate::model::Finding::new(
        crate::model::Status::Deleted,
        crate::model::Source::TrashFolder,
        media_for_name(&name),
        name,
    );
    // Cache key only; the finding is never shown.
    f.id = h.finish() >> 1;
    f.path = Some(path.display().to_string());
    crate::export::thumbnail(&f, cache)
}

/// Where a file from the clone goes on an Android phone, relative to
/// `/sdcard`. `album` names the Gallery album photos and videos land in.
pub fn android_destination(relative: &str, album: &str) -> String {
    let mut parts = relative.splitn(2, '/');
    let top = parts.next().unwrap_or("");
    let rest = parts.next().unwrap_or("");
    let name = rest.rsplit('/').next().unwrap_or(rest);
    match top {
        // Photos & Videos/<year>/<year-month>/file -> DCIM/<album>/<year-month>/file
        "Photos & Videos" => {
            let month = rest
                .split('/')
                .nth(1)
                .filter(|_| rest.matches('/').count() >= 2);
            match month {
                Some(m) => format!("DCIM/{album}/{m}/{name}"),
                None => format!("DCIM/{album}/{name}"),
            }
        }
        // Apps/<app>/<year-month>/file -> Pictures|Movies|Music|Documents/<app>/file
        "Apps" => {
            let app = rest.split('/').next().unwrap_or("App");
            let place = match media_for_name(name) {
                Media::Image => "Pictures",
                Media::Video => "Movies",
                Media::Audio => "Music",
                _ => "Documents",
            };
            format!("{place}/{app}/{name}")
        }
        "Downloads" => format!("Download/{rest}"),
        "Documents" => format!("Documents/{rest}"),
        "Files on iPhone" => format!("Documents/Files on iPhone/{rest}"),
        // Audio/<Recordings|Music|...>/... -> the same folder on the phone
        "Audio" => rest.to_string(),
        _ => format!("Documents/{name}"),
    }
}

/// The files a selection covers, as paths inside the clone, without repeats.
fn selected_files(clone: &Path, selection: &Selection) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for g in &selection.groups {
        let mut files = Vec::new();
        walk(&inside(clone, g)?, &mut files);
        out.extend(files.iter().map(|f| rel(clone, f)));
    }
    for f in &selection.files {
        let path = inside(clone, f)?;
        if path.is_file() {
            out.push(f.clone());
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// Lay the selection out as it should appear on the phone, under `staging`.
/// On APFS each copy is a clone, so this costs no space. Returns the count
/// and total size.
pub fn stage_for_android(
    clone: &Path,
    selection: &Selection,
    album: &str,
    staging: &Path,
) -> Result<(u64, u64), String> {
    let _ = fs::remove_dir_all(staging);
    let mut files = 0;
    let mut bytes = 0;
    for relative in selected_files(clone, selection)? {
        let src = clone.join(&relative);
        let dest = crate::export::unique(staging.join(android_destination(&relative, album)));
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let size =
            fs::copy(&src, &dest).map_err(|e| format!("Could not prepare {relative}: {e}"))?;
        if let Ok(modified) = fs::metadata(&src).and_then(|m| m.modified()) {
            if let Ok(f) = fs::File::options().write(true).open(&dest) {
                let _ = f.set_modified(modified);
            }
        }
        files += 1;
        bytes += size;
    }
    Ok((files, bytes))
}

/// Send a selection of a clone's files to a connected phone.
pub fn transfer(
    clone: &Path,
    device: &Device,
    selection: &Selection,
    album: &str,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(&Progress),
) -> Result<Transferred, String> {
    if device.kind != DeviceKind::Android {
        return Err(
            "Single files can only be sent to Android phones. Apple allows no app to add files to an iPhone's apps; to move everything to a new iPhone, restore it from an iPhone clone."
                .into(),
        );
    }
    let adb = find_tool("adb").ok_or("Install Android platform tools first.")?;
    let say = |progress: &mut dyn FnMut(&Progress), phase: &str| {
        progress(&Progress {
            phase: phase.into(),
            current: device.name.clone(),
            ..Progress::default()
        });
    };
    say(progress, "Preparing the files");
    let staging = clone.join(".transfer");
    let (files, bytes) = stage_for_android(clone, selection, album, &staging)?;
    if files == 0 {
        let _ = fs::remove_dir_all(&staging);
        return Err("Nothing was selected.".into());
    }
    say(
        progress,
        "Sending the files to the phone (keep it unlocked)",
    );
    let from = format!("{}/.", staging.display());
    let pushed = run_with(
        &adb,
        &["-s", &device.id, "push", &from, "/sdcard/"],
        &[],
        clone,
        cancel,
        &mut |_| {},
    );
    let _ = fs::remove_dir_all(&staging);
    pushed.map_err(|e| format!("Sending to the phone failed: {e}"))?;

    // Ask the phone to index the new photos so the Gallery shows them now
    // rather than after its next scan. Best effort: older and newer Android
    // versions answer different requests.
    say(progress, "Adding the photos to the Gallery");
    let album_dir = format!("file:///sdcard/DCIM/{album}");
    let _ = run_with(
        &adb,
        &[
            "-s",
            &device.id,
            "shell",
            "am",
            "broadcast",
            "-a",
            "android.intent.action.MEDIA_SCANNER_SCAN_FILE",
            "-d",
            &album_dir,
        ],
        &[],
        clone,
        cancel,
        &mut |_| {},
    );
    Ok(Transferred {
        files,
        bytes,
        notes: vec![format!(
            "Photos and videos are in the Gallery album \"{album}\". If they do not show yet, restart the phone."
        )],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrity::tests::temp_dir;

    fn put(root: &Path, rel: &str, size: usize) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, vec![1u8; size]).unwrap();
    }

    fn sample_clone() -> PathBuf {
        let c = temp_dir("transfer");
        put(&c, "Photos & Videos/2025/2025-03/IMG_1.HEIC", 100);
        put(&c, "Photos & Videos/2024/2024-12/IMG_0.MOV", 300);
        put(&c, "Apps/WhatsApp/2025-01/VID-1.mp4", 50);
        put(&c, "Apps/WhatsApp/2025-01/PTT-1.opus", 20);
        put(&c, "Apps/Messages/2025-02/IMG_9.jpg", 10);
        put(&c, "Downloads/Tickets/boarding.pdf", 5);
        put(&c, "Audio/Recordings/memo.m4a", 7);
        put(&c, "Restore backup/abc/Manifest.db", 999);
        fs::write(c.join("clone.json"), "{}").unwrap();
        c
    }

    #[test]
    fn lists_groups_with_apps_one_by_one() {
        let c = sample_clone();
        let g = groups(&c);
        let ids: Vec<&str> = g.iter().map(|g| g.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "Photos & Videos",
                "Apps/Messages",
                "Apps/WhatsApp",
                "Downloads",
                "Audio"
            ],
            "the restorable backup is never offered as files"
        );
        assert_eq!(g[0].files, 2);
        assert_eq!(g[0].bytes, 400);
        assert_eq!(g[2].label, "WhatsApp");
        assert_eq!(g[2].sample.len(), 1);
    }

    #[test]
    fn pages_through_a_group() {
        let c = sample_clone();
        let (page, total) = items(&c, "Photos & Videos", 0, 1).unwrap();
        assert_eq!(total, 2);
        assert_eq!(page.len(), 1);
        assert!(items(&c, "../etc", 0, 10).is_err());
        assert!(items(&c, "Restore backup", 0, 10).is_err());
    }

    #[test]
    fn android_destinations() {
        let a = "From Sam's iPhone";
        assert_eq!(
            android_destination("Photos & Videos/2025/2025-03/IMG_1.HEIC", a),
            "DCIM/From Sam's iPhone/2025-03/IMG_1.HEIC"
        );
        assert_eq!(
            android_destination("Apps/WhatsApp/2025-01/VID-1.mp4", a),
            "Movies/WhatsApp/VID-1.mp4"
        );
        assert_eq!(
            android_destination("Apps/WhatsApp/2025-01/PTT-1.opus", a),
            "Music/WhatsApp/PTT-1.opus"
        );
        assert_eq!(
            android_destination("Downloads/Tickets/boarding.pdf", a),
            "Download/Tickets/boarding.pdf"
        );
        assert_eq!(
            android_destination("Audio/Recordings/memo.m4a", a),
            "Recordings/memo.m4a"
        );
        assert_eq!(
            android_destination("Files on iPhone/Taxes/2024.pdf", a),
            "Documents/Files on iPhone/Taxes/2024.pdf"
        );
    }

    #[test]
    fn stages_a_mixed_selection_once_each() {
        let c = sample_clone();
        let staging = c.join(".transfer");
        let selection = Selection {
            groups: vec!["Apps/WhatsApp".into()],
            files: vec![
                "Photos & Videos/2025/2025-03/IMG_1.HEIC".into(),
                "Apps/WhatsApp/2025-01/VID-1.mp4".into(),
            ],
        };
        let (files, bytes) = stage_for_android(&c, &selection, "From old phone", &staging).unwrap();
        assert_eq!((files, bytes), (3, 170));
        assert!(staging
            .join("DCIM/From old phone/2025-03/IMG_1.HEIC")
            .is_file());
        assert!(staging.join("Movies/WhatsApp/VID-1.mp4").is_file());
        assert!(staging.join("Music/WhatsApp/PTT-1.opus").is_file());
        assert!(
            !staging.join("Download").exists(),
            "only the selection is staged"
        );

        let escape = Selection {
            groups: vec![],
            files: vec!["Restore backup/abc/Manifest.db".into()],
        };
        assert!(stage_for_android(&c, &escape, "x", &staging).is_err());
    }

    #[test]
    fn single_files_cannot_go_to_an_iphone() {
        let c = sample_clone();
        let iphone = Device {
            id: "u".into(),
            kind: DeviceKind::Iphone,
            name: "New iPhone".into(),
            detail: String::new(),
            path: None,
            problem: None,
        };
        let err = transfer(
            &c,
            &iphone,
            &Selection::default(),
            "x",
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .unwrap_err();
        assert!(err.contains("restore it from an iPhone clone"));
    }
}
