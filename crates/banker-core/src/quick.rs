//! The quick photo scan of a phone: only the deleted photos and videos are
//! fetched, never a copy of the whole phone, so it works on a Mac with
//! little free space.
//!
//! - Android: the phone is asked (over adb) where its trashed files are, and
//!   only those are pulled.
//! - iPhone: the photo library's database is read over Apple's file service
//!   (AFC, the one Image Capture uses), and only the photos it marks as
//!   Recently Deleted are fetched.
//!
//! The files land in a small folder that the normal scanner then reads.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use crate::devices::{find_tool, pair_iphone, run, Device, DeviceKind};
use crate::ios;
use crate::scan::Progress;
use crate::settings::check_space;
use crate::sqlite::Db;

/// The folder an iPhone's deleted photos are put in, shaped like a Photos
/// library so the scanner reads it the same way.
const IPHONE_LIBRARY: &str = "iPhone.photoslibrary";

/// Where a phone's quick scan keeps what it fetched.
pub fn quick_dir(work: &Path, device: &Device) -> PathBuf {
    let safe: String = device
        .id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    work.join("Quick photo scans").join(safe)
}

/// `find ... -exec stat -c '%s %n'` output: (size, path) for each file.
pub fn parse_sized_paths(out: &str) -> Vec<(u64, String)> {
    out.lines()
        .filter_map(|l| {
            let (size, path) = l.trim_end().split_once(' ')?;
            Some((size.parse().ok()?, path.to_string()))
        })
        .filter(|(_, p)| p.starts_with('/'))
        .collect()
}

/// `afcclient info` output: the file's size.
pub fn parse_afc_size(out: &str) -> Option<u64> {
    out.lines().find_map(|l| {
        let (key, value) = l.split_once(':')?;
        (key.trim() == "st_size")
            .then(|| value.trim().parse().ok())
            .flatten()
    })
}

/// Android's trash, in the places phones keep it: `.trashed-*` files left by
/// MediaStore, `.Trash` folders (Samsung and others) and `LOST.DIR`.
const ANDROID_FIND: &str = "find /sdcard/ \\( -name '.trashed-*' -o -path '*/.Trash/*' -o -path '*/.trash/*' -o -path '*/LOST.DIR/*' \\) -type f -exec stat -c '%s %n' {} + 2>/dev/null";

/// Fetch a phone's deleted photos into `quick_dir` and return that folder.
pub fn fetch(
    device: &Device,
    work: &Path,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(&Progress),
) -> Result<PathBuf, String> {
    let dest = quick_dir(work, device);
    let _ = fs::remove_dir_all(&dest);
    fs::create_dir_all(&dest).map_err(|e| format!("Could not create {}: {e}", dest.display()))?;
    match device.kind {
        DeviceKind::Android => android(device, &dest, cancel, progress)?,
        DeviceKind::Iphone => iphone(device, &dest, cancel, progress)?,
        _ => {
            return Err(
                "A quick photo scan is for phones. Drives and this Mac are read where they are."
                    .into(),
            )
        }
    }
    Ok(dest)
}

fn step(progress: &mut dyn FnMut(&Progress), phase: &str, done: usize, total: usize) {
    progress(&Progress {
        phase: phase.into(),
        files: done as u64,
        percent: (total > 0).then(|| (done * 100 / total) as u8),
        ..Progress::default()
    });
}

fn android(
    device: &Device,
    dest: &Path,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(&Progress),
) -> Result<(), String> {
    let adb = find_tool("adb").ok_or("Reco cannot find its Android tools. Reinstall Reco.")?;
    step(progress, "Asking the phone for its deleted photos", 0, 0);
    let listed = run(
        &adb,
        &["-s", &device.id, "shell", ANDROID_FIND],
        dest,
        cancel,
    )
    // find exits non-zero when a folder is unreadable; what it printed counts.
    .or_else(|e| if e == "Stopped." { Err(e) } else { Ok(e) })?;
    let log = fs::read_to_string(dest.join("banker-copy-log.txt")).unwrap_or(listed);
    let files = parse_sized_paths(&log);
    let _ = fs::remove_file(dest.join("banker-copy-log.txt"));
    let total: u64 = files.iter().map(|(s, _)| s).sum();
    check_space(dest, "the deleted photos", Some(total), 0)?;
    for (i, (_, remote)) in files.iter().enumerate() {
        step(
            progress,
            "Fetching deleted photos from the phone",
            i,
            files.len(),
        );
        let rel = remote
            .trim_start_matches("/sdcard/")
            .trim_start_matches('/');
        let local = dest.join("sdcard").join(rel);
        if let Some(parent) = local.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let target = local.display().to_string();
        match run(
            &adb,
            &["-s", &device.id, "pull", "-a", remote, &target],
            dest,
            cancel,
        ) {
            Err(e) if e == "Stopped." => return Err(e),
            _ => {}
        }
    }
    let _ = fs::remove_file(dest.join("banker-copy-log.txt"));
    Ok(())
}

fn iphone(
    device: &Device,
    dest: &Path,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(&Progress),
) -> Result<(), String> {
    let afc = find_tool("afcclient").ok_or(
        "This copy of Reco cannot read iPhone photos directly. Use Deep scan instead, or install the latest Reco.",
    )?;
    pair_iphone(&device.id, dest, cancel)?;
    step(progress, "Reading the iPhone's photo library", 0, 0);
    let library = dest.join(IPHONE_LIBRARY);
    let db_dir = library.join("database");
    fs::create_dir_all(&db_dir).map_err(|e| e.to_string())?;
    let get = |remote: &str, local: &Path| {
        let target = local.display().to_string();
        run(
            &afc,
            &["-u", &device.id, "get", remote, &target],
            dest,
            cancel,
        )
    };
    get("/PhotoData/Photos.sqlite", &db_dir.join("Photos.sqlite")).map_err(|e| {
        if e == "Stopped." {
            e
        } else {
            "The iPhone did not share its photo library. Unlock it, keep it unlocked, and try again.".to_string()
        }
    })?;
    let _ = get(
        "/PhotoData/Photos.sqlite-wal",
        &db_dir.join("Photos.sqlite-wal"),
    );

    let data = fs::read(db_dir.join("Photos.sqlite")).map_err(|e| e.to_string())?;
    let mut db = Db::parse(data).map_err(|_| "The iPhone's photo library could not be read.")?;
    if let Ok(wal) = fs::read(db_dir.join("Photos.sqlite-wal")) {
        db.apply_wal(&wal);
    }
    let assets = ios::trashed_assets(&db);

    let mut sized = Vec::new();
    for a in &assets {
        let remote = format!("/{}/{}", a.directory, a.filename);
        let size = run(&afc, &["-u", &device.id, "info", &remote], dest, cancel)
            .ok()
            .and_then(|o| parse_afc_size(&o));
        sized.push((remote, a, size));
    }
    let total: u64 = sized.iter().filter_map(|(_, _, s)| *s).sum();
    check_space(dest, "the deleted photos", Some(total), 0)?;
    for (i, (remote, a, size)) in sized.iter().enumerate() {
        step(
            progress,
            "Fetching deleted photos from the iPhone",
            i,
            sized.len(),
        );
        if size.is_none() {
            // The original is only in iCloud. The phone keeps a large
            // preview; put it where the scanner looks for previews.
            if let Some(uuid) = &a.uuid {
                let thumb = format!(
                    "/PhotoData/Thumbnails/V2/{}/{}/5005.JPG",
                    a.directory, a.filename
                );
                let local = library
                    .join("resources/derivatives")
                    .join(format!("{uuid}_5005.jpeg"));
                if let Some(parent) = local.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if let Err(e) = get(&thumb, &local) {
                    if e == "Stopped." {
                        return Err(e);
                    }
                }
            }
            continue;
        }
        let local = library
            .join("originals")
            .join(&a.directory)
            .join(&a.filename);
        if let Some(parent) = local.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Err(e) = get(remote, &local) {
            if e == "Stopped." {
                return Err(e);
            }
        }
    }
    let _ = fs::remove_file(dest.join("banker-copy-log.txt"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_find_output_with_spaces() {
        let out = "2048 /sdcard/DCIM/Camera/.trashed-1745000000-IMG 1.jpg\n\
                   find: /sdcard/Android/data: Permission denied\n\
                   10 /sdcard/Android/.Trash/com.sec.android.gallery3d/1/IMG_2.jpg\n";
        assert_eq!(
            parse_sized_paths(out),
            vec![
                (
                    2048,
                    "/sdcard/DCIM/Camera/.trashed-1745000000-IMG 1.jpg".into()
                ),
                (
                    10,
                    "/sdcard/Android/.Trash/com.sec.android.gallery3d/1/IMG_2.jpg".into()
                ),
            ]
        );
    }

    #[test]
    fn parses_afc_info() {
        let out = "st_size: 4194304\nst_blocks: 8192\nst_ifmt: S_IFREG\n";
        assert_eq!(parse_afc_size(out), Some(4_194_304));
        assert_eq!(parse_afc_size("ERROR: not found"), None);
    }

    #[test]
    fn quick_folders_are_per_phone() {
        let device = Device {
            id: "00008030-001A:2B".into(),
            kind: DeviceKind::Iphone,
            name: "Sam's iPhone".into(),
            detail: String::new(),
            path: None,
            problem: None,
        };
        assert_eq!(
            quick_dir(Path::new("/w"), &device),
            Path::new("/w/Quick photo scans/00008030-001A2B")
        );
    }
}
