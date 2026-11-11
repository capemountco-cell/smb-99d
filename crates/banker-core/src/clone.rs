//! "Clone this device": copy a whole phone to the Mac, organised, in a form
//! that can restore it later.
//!
//! A clone folder in `~/Banker Backups/` holds:
//!
//! * `Restore backup/`: the restorable copy. For an iPhone this is a
//!   standard Apple backup (the format Finder makes), which restores the
//!   whole phone, onto it or onto another iPhone. For Android it is shared
//!   storage as copied by `adb`, which can be copied back; Android offers no
//!   way to restore apps, settings or messages without root.
//! * `Photos & Videos/<year>/<year-month>/`, `Apps/<app>/<year-month>/`,
//!   `Files on iPhone/`, `Downloads/`, `Documents/`, `Audio/`: the same
//!   content, organised to browse. On APFS these copies are clones and take
//!   no extra space.
//! * optionally `<name>.zip`, the restorable copy in one file, which Banker
//!   can restore from too.
//! * `clone.json`, which describes the clone so Banker can list it.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::devices::{
    android_pull_outcome, backup_folder, find_tool, last_percent, output, pair_iphone, run_with,
    Device, DeviceKind,
};
use crate::export::{clean, unique};
use crate::ios::{self, Manifest};
use crate::model::{media_for_name, Media};
use crate::paths::{app_display_name, app_for_ios, app_for_path};
use crate::scan::Progress;
use crate::timeline::civil_from_days;

pub const BACKUP_DIR: &str = "Restore backup";
pub const INFO_FILE: &str = "clone.json";

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct CloneOptions {
    /// iPhone only: a backup password. With one, the clone also holds saved
    /// passwords, Health, Wi-Fi and call history, and restores them.
    pub password: Option<String>,
    /// Also write the restorable copy as one .zip file.
    pub make_zip: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CloneInfo {
    pub version: u32,
    pub device_name: String,
    pub kind: DeviceKind,
    /// UDID or adb serial of the device that was cloned.
    pub device_id: String,
    pub model: Option<String>,
    pub os_version: Option<String>,
    /// Unix seconds.
    pub created: i64,
    /// The iPhone backup needs its password to restore.
    pub encrypted: bool,
    pub zip: Option<String>,
    pub photos: u64,
    pub app_files: u64,
    pub other_files: u64,
    pub bytes: u64,
    pub notes: Vec<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CloneEntry {
    pub folder: String,
    pub info: CloneInfo,
    pub backup_present: bool,
    pub zip_present: bool,
    /// The folder Banker can scan for deleted items, when the clone can be read.
    pub scan_path: Option<String>,
}

#[derive(Default, Debug, PartialEq)]
pub struct Organised {
    pub photos: u64,
    pub app_files: u64,
    pub other_files: u64,
}

fn month(t: Option<i64>) -> (String, String) {
    match t {
        Some(t) => {
            let (y, m, _) = civil_from_days(t.div_euclid(86_400));
            (y.to_string(), format!("{y}-{m:02}"))
        }
        None => ("Undated".into(), "Undated".into()),
    }
}

/// Copy `src` into `dir` under `name`, keeping its date. On APFS `fs::copy`
/// makes a clone, which costs no space.
fn place(src: &Path, dir: &Path, name: &str, modified: Option<i64>) -> bool {
    if fs::create_dir_all(dir).is_err() {
        return false;
    }
    let dest = unique(dir.join(clean(name)));
    if fs::copy(src, &dest).is_err() {
        return false;
    }
    if let Some(t) = modified.and_then(|t| u64::try_from(t).ok()) {
        if let Ok(f) = fs::File::options().write(true).open(&dest) {
            let _ = f.set_modified(UNIX_EPOCH + Duration::from_secs(t));
        }
    }
    true
}

/// Paths that hold caches and thumbnails, not anything a person made.
fn junk(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    [
        "/caches/",
        "/tmp/",
        "thumbnail",
        "/.thumbnails/",
        "/cache/",
        "/.trashed-",
    ]
    .iter()
    .any(|k| lower.contains(k))
        || lower.starts_with("library/caches")
}

fn worth_keeping(media: Media) -> bool {
    matches!(
        media,
        Media::Image | Media::Video | Media::Audio | Media::Document
    )
}

/// Sort an iPhone backup's photos, videos and app files into folders.
pub fn organise_ios(root: &Path, out: &Path) -> Result<Organised, String> {
    let entries = match ios::read_manifest(root) {
        Manifest::Entries(e) => e,
        Manifest::Encrypted => return Err("encrypted".into()),
        Manifest::Unreadable => return Err("The backup's index could not be read.".into()),
    };
    let mut n = Organised::default();
    for e in entries {
        let src = e.stored_at(root);
        if !src.is_file() {
            continue;
        }
        let rel = e.relative_path.as_str();
        let name = rel.rsplit('/').next().unwrap_or(rel);
        let media = media_for_name(name);
        let (year, ym) = month(e.modified);
        let size = e.size.unwrap_or(0);
        if e.domain == "CameraRollDomain" && rel.starts_with("Media/DCIM/") {
            if matches!(media, Media::Image | Media::Video)
                && place(
                    &src,
                    &out.join("Photos & Videos").join(&year).join(&ym),
                    name,
                    e.modified,
                )
            {
                n.photos += 1;
            }
        } else if let Some(rest) = (e.domain
            == "AppDomainGroup-group.com.apple.FileProvider.LocalStorage")
            .then(|| rel.strip_prefix("File Provider Storage/"))
            .flatten()
        {
            let sub = Path::new(rest).parent().unwrap_or(Path::new(""));
            if place(
                &src,
                &out.join("Files on iPhone").join(sub),
                name,
                e.modified,
            ) {
                n.other_files += 1;
            }
        } else if let Some(app) = app_for_ios(&e.domain, rel) {
            if worth_keeping(media) && size >= 10_000 && !junk(&format!("/{rel}")) {
                let dir = out
                    .join("Apps")
                    .join(clean(&app_display_name(&app)))
                    .join(&ym);
                if place(&src, &dir, name, e.modified) {
                    n.app_files += 1;
                }
            }
        }
    }
    Ok(n)
}

fn mtime(meta: &fs::Metadata) -> Option<i64> {
    let d = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    i64::try_from(d.as_secs()).ok()
}

/// Sort a copy of an Android phone's shared storage into folders.
pub fn organise_android(sdcard: &Path, out: &Path) -> Organised {
    let mut n = Organised::default();
    let mut stack = vec![sdcard.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            let rel = path
                .strip_prefix(sdcard)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if name.starts_with('.') || rel == "Android/data" || rel == "Android/obb" {
                continue;
            }
            if meta.is_dir() {
                stack.push(path);
                continue;
            }
            if !meta.is_file() || junk(&format!("/{rel}")) {
                continue;
            }
            let media = media_for_name(&name);
            let modified = mtime(&meta);
            let (year, ym) = month(modified);
            let top = rel.split('/').next().unwrap_or("");
            let rest_dir = Path::new(&rel)
                .parent()
                .and_then(|p| p.strip_prefix(top).ok())
                .unwrap_or(Path::new(""))
                .to_path_buf();
            let app = app_for_path(&rel).filter(|a| a.contains('.') || a == "Screenshots");
            let placed = match (top, app) {
                ("DCIM" | "Pictures" | "Movies", None)
                    if matches!(media, Media::Image | Media::Video) =>
                {
                    place(
                        &path,
                        &out.join("Photos & Videos").join(&year).join(&ym),
                        &name,
                        modified,
                    )
                    .then(|| n.photos += 1)
                }
                (_, Some(app)) if worth_keeping(media) => place(
                    &path,
                    &out.join("Apps")
                        .join(clean(&app_display_name(&app)))
                        .join(&ym),
                    &name,
                    modified,
                )
                .then(|| n.app_files += 1),
                ("Download", _) => place(
                    &path,
                    &out.join("Downloads").join(&rest_dir),
                    &name,
                    modified,
                )
                .then(|| n.other_files += 1),
                ("Documents", _) => place(
                    &path,
                    &out.join("Documents").join(&rest_dir),
                    &name,
                    modified,
                )
                .then(|| n.other_files += 1),
                ("Music" | "Recordings" | "Podcasts" | "Ringtones" | "Notifications", _) => place(
                    &path,
                    &out.join("Audio").join(top).join(&rest_dir),
                    &name,
                    modified,
                )
                .then(|| n.other_files += 1),
                _ => None,
            };
            let _ = placed;
        }
    }
    n
}

fn dir_size(dir: &Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = fs::read_dir(&d) else {
            continue;
        };
        for e in entries.filter_map(Result::ok) {
            match e.file_type() {
                Ok(t) if t.is_dir() => stack.push(e.path()),
                Ok(t) if t.is_file() => total += e.metadata().map_or(0, |m| m.len()),
                _ => {}
            }
        }
    }
    total
}

fn ios_value(udid: &str, key: &str) -> Option<String> {
    let tool = find_tool("ideviceinfo")?;
    output(&tool, &["-u", udid, "-k", key])
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && !s.contains("ERROR"))
}

fn write_info(folder: &Path, info: &CloneInfo) -> Result<(), String> {
    let json = serde_json::to_string_pretty(info).map_err(|e| e.to_string())?;
    fs::write(folder.join(INFO_FILE), json)
        .map_err(|e| format!("Could not save the clone's details: {e}"))
}

fn entry(folder: &Path, info: CloneInfo) -> CloneEntry {
    let backup = folder.join(BACKUP_DIR);
    let scan_path = match info.kind {
        DeviceKind::Iphone if !info.encrypted => ios::backup_root(&backup),
        DeviceKind::Android => Some(backup.clone()).filter(|b| b.is_dir()),
        _ => None,
    };
    CloneEntry {
        folder: folder.display().to_string(),
        backup_present: backup.is_dir(),
        zip_present: info.zip.as_ref().is_some_and(|z| folder.join(z).is_file()),
        scan_path: scan_path.map(|p| p.display().to_string()),
        info,
    }
}

/// Every clone in `~/Banker Backups`, newest first.
pub fn list_clones(home: &Path) -> Vec<CloneEntry> {
    let Ok(dirs) = fs::read_dir(home.join("Banker Backups")) else {
        return Vec::new();
    };
    let mut clones: Vec<CloneEntry> = dirs
        .filter_map(Result::ok)
        .filter_map(|d| {
            let folder = d.path();
            let text = fs::read_to_string(folder.join(INFO_FILE)).ok()?;
            let info: CloneInfo = serde_json::from_str(&text).ok()?;
            Some(entry(&folder, info))
        })
        .collect();
    clones.sort_by_key(|c| std::cmp::Reverse(c.info.created));
    clones
}

/// Press "Clone this device": copy, organise, optionally zip, describe.
pub fn clone_device(
    device: &Device,
    home: &Path,
    now: i64,
    tz_offset_minutes: i32,
    options: &CloneOptions,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(&Progress),
) -> Result<CloneEntry, String> {
    let local = now + tz_offset_minutes as i64 * 60;
    let folder = backup_folder(home, &format!("{} clone", device.name), local);
    let backup = folder.join(BACKUP_DIR);
    fs::create_dir_all(&backup)
        .map_err(|e| format!("Could not create {}: {e}", backup.display()))?;
    let say = |progress: &mut dyn FnMut(&Progress), phase: &str, percent: Option<u8>| {
        progress(&Progress {
            phase: phase.into(),
            current: folder.display().to_string(),
            percent,
            ..Progress::default()
        });
    };
    let password = options.password.as_deref().filter(|p| !p.is_empty());
    let mut info = CloneInfo {
        version: 1,
        device_name: device.name.clone(),
        kind: device.kind,
        device_id: device.id.clone(),
        model: None,
        os_version: None,
        created: now,
        encrypted: false,
        zip: None,
        photos: 0,
        app_files: 0,
        other_files: 0,
        bytes: 0,
        notes: Vec::new(),
    };

    match device.kind {
        DeviceKind::Iphone => {
            let tool = find_tool("idevicebackup2").ok_or("Install libimobiledevice first.")?;
            say(progress, "Connecting to the iPhone", None);
            pair_iphone(&device.id, &folder, cancel)?;
            info.model = ios_value(&device.id, "ProductType");
            info.os_version = ios_value(&device.id, "ProductVersion");
            if let Some(pw) = password {
                // Turns on encrypted backups on the phone itself, as Finder's
                // "Encrypt local backup" does. Already on: keep going, the
                // phone's existing password applies.
                let on = run_with(
                    &tool,
                    &["-u", &device.id, "encryption", "on", pw],
                    &[],
                    &folder,
                    cancel,
                    &mut |_| {},
                );
                if let Err(e) = on {
                    if !e.to_ascii_lowercase().contains("already") {
                        return Err(format!("Could not set the backup password: {e}"));
                    }
                }
            }
            say(progress, "Cloning the iPhone (keep it unlocked)", Some(0));
            let env: Vec<(&str, &str)> = password
                .map(|p| ("BACKUP_PASSWORD", p))
                .into_iter()
                .collect();
            let target = backup.display().to_string();
            run_with(
                &tool,
                &["-u", &device.id, "backup", "--full", &target],
                &env,
                &folder,
                cancel,
                &mut |text| {
                    if let Some(p) = last_percent(text) {
                        say(progress, "Cloning the iPhone (keep it unlocked)", Some(p));
                    }
                },
            )
            .map_err(|e| format!("Cloning the iPhone failed: {e}"))?;
            let root = backup.join(&device.id);
            say(progress, "Organising photos, videos and files", None);
            match organise_ios(&root, &folder) {
                Ok(n) => {
                    info.photos = n.photos;
                    info.app_files = n.app_files;
                    info.other_files = n.other_files;
                }
                Err(e) if e == "encrypted" => {
                    info.encrypted = true;
                    info.notes.push(
                        "The backup is protected by a password, so its photos and files were not sorted into folders. It restores the whole iPhone, passwords included, with that password."
                            .into(),
                    );
                }
                Err(e) => info.notes.push(e),
            }
        }
        DeviceKind::Android => {
            let adb = find_tool("adb").ok_or("Install Android platform tools first.")?;
            say(
                progress,
                "Copying the phone to this Mac (keep it unlocked)",
                None,
            );
            let target = backup.display().to_string();
            let pulled = run_with(
                &adb,
                &["-s", &device.id, "pull", "-a", "/sdcard/", &target],
                &[],
                &folder,
                cancel,
                &mut |_| {},
            );
            if let Some(warning) = android_pull_outcome(pulled, &backup)? {
                info.notes.push(warning);
            }
            info.notes.push(
                "Android only allows files to be copied: photos, videos, downloads, documents and app media. Apps, settings and messages are not part of the clone."
                    .into(),
            );
            say(progress, "Organising photos, videos and files", None);
            let n = organise_android(&backup.join("sdcard"), &folder);
            info.photos = n.photos;
            info.app_files = n.app_files;
            info.other_files = n.other_files;
        }
        _ => return Err("Only phones can be cloned. Drives are read where they are.".into()),
    }

    info.bytes = dir_size(&backup);
    if options.make_zip {
        say(progress, "Making the .zip", None);
        let (y, m, d) = civil_from_days(local.div_euclid(86_400));
        let zip_name = format!("{} {y}-{m:02}-{d:02}.zip", clean(&device.name));
        let zipped = run_with(
            Path::new("/usr/bin/ditto"),
            &[
                "-c",
                "-k",
                "--sequesterRsrc",
                "--keepParent",
                &backup.display().to_string(),
                &folder.join(&zip_name).display().to_string(),
            ],
            &[],
            &folder,
            cancel,
            &mut |_| {},
        );
        match zipped {
            Ok(_) => info.zip = Some(zip_name),
            Err(e) => info.notes.push(format!("The .zip could not be made: {e}")),
        }
    }
    write_info(&folder, &info)?;
    Ok(entry(&folder, info))
}

/// The restorable copy of a clone: its folder, or unzipped from its .zip.
fn restorable(clone: &Path, info: &CloneInfo, cancel: &AtomicBool) -> Result<PathBuf, String> {
    let backup = clone.join(BACKUP_DIR);
    if backup.is_dir() {
        return Ok(backup);
    }
    let zip = info
        .zip
        .as_ref()
        .map(|z| clone.join(z))
        .filter(|z| z.is_file())
        .ok_or("This clone's restorable copy and its .zip are both gone.")?;
    let unzipped = clone.join("Unzipped for restore");
    let _ = fs::remove_dir_all(&unzipped);
    run_with(
        Path::new("/usr/bin/ditto"),
        &[
            "-x",
            "-k",
            &zip.display().to_string(),
            &unzipped.display().to_string(),
        ],
        &[],
        clone,
        cancel,
        &mut |_| {},
    )
    .map_err(|e| format!("The .zip could not be opened: {e}"))?;
    let inner = unzipped.join(BACKUP_DIR);
    Ok(if inner.is_dir() { inner } else { unzipped })
}

/// Restore a clone onto a connected phone of the same kind. For an iPhone
/// this erases it and restores the whole phone from the clone.
pub fn restore(
    clone: &Path,
    device: &Device,
    password: Option<&str>,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(&Progress),
) -> Result<String, String> {
    let text = fs::read_to_string(clone.join(INFO_FILE))
        .map_err(|_| "This folder is not a Banker clone.")?;
    let info: CloneInfo = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if info.kind != device.kind {
        return Err("A clone can only be restored to the same kind of phone.".into());
    }
    let say = |progress: &mut dyn FnMut(&Progress), phase: &str, percent: Option<u8>| {
        progress(&Progress {
            phase: phase.into(),
            current: clone.display().to_string(),
            percent,
            ..Progress::default()
        });
    };
    say(progress, "Preparing the clone", None);
    let source = restorable(clone, &info, cancel)?;

    match device.kind {
        DeviceKind::Iphone => {
            let tool = find_tool("idevicebackup2").ok_or("Install libimobiledevice first.")?;
            let password = password.filter(|p| !p.is_empty());
            if info.encrypted && password.is_none() {
                return Err("This clone is protected by a password. Enter it to restore.".into());
            }
            pair_iphone(&device.id, clone, cancel)?;
            let find_my = find_tool("ideviceinfo")
                .and_then(|t| {
                    output(
                        &t,
                        &[
                            "-u",
                            &device.id,
                            "-q",
                            "com.apple.fmip",
                            "-k",
                            "IsAssociated",
                        ],
                    )
                })
                .unwrap_or_default();
            if find_my.trim() == "true" {
                return Err(
                    "Turn off Find My on the iPhone first (Settings > your name > Find My > Find My iPhone), then try again."
                        .into(),
                );
            }
            say(
                progress,
                "Restoring the iPhone (keep it plugged in)",
                Some(0),
            );
            let env: Vec<(&str, &str)> = password
                .map(|p| ("BACKUP_PASSWORD", p))
                .into_iter()
                .collect();
            run_with(
                &tool,
                &[
                    "-u",
                    &device.id,
                    "-s",
                    &info.device_id,
                    "restore",
                    "--system",
                    "--settings",
                    "--reboot",
                    "--copy",
                    &source.display().to_string(),
                ],
                &env,
                clone,
                cancel,
                &mut |text| {
                    if let Some(p) = last_percent(text) {
                        say(
                            progress,
                            "Restoring the iPhone (keep it plugged in)",
                            Some(p),
                        );
                    }
                },
            )
            .map_err(|e| format!("Restoring the iPhone failed: {e}"))?;
            Ok("The iPhone is restoring. It restarts and finishes on its own; keep it plugged in until you see the Home Screen. Apps download again from the App Store with their data.".into())
        }
        DeviceKind::Android => {
            let adb = find_tool("adb").ok_or("Install Android platform tools first.")?;
            say(progress, "Copying the files back to the phone", None);
            let from = format!("{}/sdcard/.", source.display());
            run_with(
                &adb,
                &["-s", &device.id, "push", &from, "/sdcard/"],
                &[],
                clone,
                cancel,
                &mut |_| {},
            )
            .map_err(|e| format!("Copying back to the phone failed: {e}"))?;
            Ok("The clone's photos, videos and files were copied back to the phone.".into())
        }
        _ => Err("Only phones can be restored.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrity::tests::{temp_dir, tiny_jpeg};

    fn days(y: i64, m: u32, d: u32) -> i64 {
        crate::timeline::days_from_civil(y, m, d) * 86_400
    }

    fn put(root: &Path, rel: &str, data: &[u8], when: i64) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, data).unwrap();
        let f = fs::File::options().write(true).open(&path).unwrap();
        f.set_modified(UNIX_EPOCH + Duration::from_secs(when as u64))
            .unwrap();
    }

    #[test]
    fn organises_an_android_copy() {
        let dir = temp_dir("organise-android");
        let sd = dir.join("sdcard");
        let out = dir.join("clone");
        let j = tiny_jpeg();
        let big = vec![7u8; 20_000];
        put(&sd, "DCIM/Camera/IMG_1.jpg", &j, days(2025, 3, 14));
        put(&sd, "Pictures/Screenshots/shot.png", &j, days(2024, 12, 1));
        put(&sd, "DCIM/.thumbnails/1.jpg", &j, days(2025, 3, 14));
        put(
            &sd,
            "Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Video/VID-1.mp4",
            &big,
            days(2025, 1, 2),
        );
        put(
            &sd,
            "Android/data/com.secret/files/x.db",
            b"x",
            days(2025, 1, 2),
        );
        put(
            &sd,
            "Download/Tickets/boarding.pdf",
            b"%PDF",
            days(2025, 2, 1),
        );
        put(&sd, "Recordings/memo.m4a", &big, days(2025, 2, 1));

        let n = organise_android(&sd, &out);
        assert!(out.join("Photos & Videos/2025/2025-03/IMG_1.jpg").is_file());
        assert!(out.join("Apps/Screenshots/2024-12/shot.png").is_file());
        assert!(out.join("Apps/WhatsApp/2025-01/VID-1.mp4").is_file());
        assert!(out.join("Downloads/Tickets/boarding.pdf").is_file());
        assert!(out.join("Audio/Recordings/memo.m4a").is_file());
        assert!(
            !out.join("Photos & Videos/2025/2025-03/1.jpg").exists(),
            "thumbnails are skipped"
        );
        assert_eq!(
            n,
            Organised {
                photos: 1,
                app_files: 2,
                other_files: 2
            }
        );
        let kept = fs::metadata(out.join("Photos & Videos/2025/2025-03/IMG_1.jpg")).unwrap();
        assert_eq!(mtime(&kept), Some(days(2025, 3, 14)), "dates are kept");
    }

    #[test]
    fn organises_an_iphone_backup() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ios-backup");
        let out = temp_dir("organise-ios");
        let n = organise_ios(&fixture, &out).unwrap();
        assert!(out
            .join("Photos & Videos/2025/2025-03/IMG_0001.HEIC")
            .is_file());
        assert!(out.join("Apps/WhatsApp/2025-01/VID-1.mp4").is_file());
        assert!(out.join("Files on iPhone/Taxes/2024.pdf").is_file());
        assert!(
            !out.join("Apps/WhatsApp/2025-01/big.jpg").exists(),
            "caches are skipped"
        );
        assert!(
            !out.join("Photos & Videos/2025/2025-03/x.jpg").exists(),
            "thumbnails are skipped"
        );
        assert_eq!(
            n,
            Organised {
                photos: 1,
                app_files: 1,
                other_files: 1
            }
        );
        let kept = fs::metadata(out.join("Photos & Videos/2025/2025-03/IMG_0001.HEIC")).unwrap();
        assert_eq!(
            mtime(&kept),
            Some(1_741_953_600),
            "the photo keeps the date it was taken"
        );
    }

    #[test]
    fn clones_are_listed_newest_first_and_know_what_they_can_do() {
        let home = temp_dir("clones");
        let make = |name: &str, created: i64, kind: DeviceKind, encrypted: bool| {
            let folder = home.join("Banker Backups").join(name);
            fs::create_dir_all(folder.join(BACKUP_DIR)).unwrap();
            let info = CloneInfo {
                version: 1,
                device_name: name.into(),
                kind,
                device_id: "id".into(),
                model: None,
                os_version: None,
                created,
                encrypted,
                zip: Some("x.zip".into()),
                photos: 3,
                app_files: 0,
                other_files: 0,
                bytes: 10,
                notes: vec![],
            };
            write_info(&folder, &info).unwrap();
        };
        make("Old phone", 100, DeviceKind::Android, false);
        make("New phone", 200, DeviceKind::Iphone, true);
        fs::create_dir_all(home.join("Banker Backups/not a clone")).unwrap();

        let list = list_clones(&home);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].info.device_name, "New phone");
        assert!(list[0].backup_present);
        assert!(!list[0].zip_present);
        assert_eq!(
            list[0].scan_path, None,
            "encrypted clones cannot be scanned"
        );
        assert!(list[1].scan_path.as_deref().unwrap().ends_with(BACKUP_DIR));
    }

    #[test]
    fn restore_refuses_the_wrong_kind_of_phone() {
        let home = temp_dir("restore-kind");
        let folder = home.join("c");
        fs::create_dir_all(folder.join(BACKUP_DIR)).unwrap();
        let info = CloneInfo {
            version: 1,
            device_name: "Pixel".into(),
            kind: DeviceKind::Android,
            device_id: "s".into(),
            model: None,
            os_version: None,
            created: 0,
            encrypted: false,
            zip: None,
            photos: 0,
            app_files: 0,
            other_files: 0,
            bytes: 0,
            notes: vec![],
        };
        write_info(&folder, &info).unwrap();
        let iphone = Device {
            id: "u".into(),
            kind: DeviceKind::Iphone,
            name: "iPhone".into(),
            detail: String::new(),
            path: None,
            problem: None,
        };
        let cancel = AtomicBool::new(false);
        let err = restore(&folder, &iphone, None, &cancel, &mut |_| {}).unwrap_err();
        assert!(err.contains("same kind"));
    }
}
