//! Walk a device (a mounted drive, a copied phone folder or an iPhone backup)
//! and collect everything deleted or damaged.

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

use crate::devices::{backup_folder, copy_to_mac, Device, DeviceKind};
use crate::integrity;
use crate::ios::{self, Manifest};
use crate::model::{media_for_name, Finding, Media, Report, Source, Status};
use crate::paths::{app_for_ios, app_for_path, trashed};
use crate::sqlite::{self, Db};

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub phase: String,
    pub files: u64,
    pub findings: usize,
    pub current: String,
}

/// Databases are read into memory whole; larger ones are skipped.
const MAX_DB_BYTES: u64 = 768 * 1024 * 1024;
const MAX_FINDINGS: usize = 250_000;

/// Folders that are system bookkeeping on a Mac volume, never user content.
const SKIP_DIRS: &[&str] = &[
    ".Spotlight-V100",
    ".fseventsd",
    ".DocumentRevisions-V100",
    ".MobileBackups",
    "System Volume Information",
];

pub struct Scanner<'a> {
    pub now: i64,
    /// Where rebuilt pictures are written.
    pub out: PathBuf,
    pub cancel: &'a AtomicBool,
    pub progress: &'a mut dyn FnMut(&Progress),
}

/// Where a file in an iPhone backup came from: its manifest entry, every
/// entry by (domain, path) to find its WAL, and the backup folder.
type IosContext<'a> = (
    &'a ios::Entry,
    &'a HashMap<(String, String), ios::Entry>,
    &'a Path,
);

struct DbContext<'a> {
    name: &'a str,
    app: Option<String>,
    path: &'a Path,
    original: Option<String>,
}

fn mtime(meta: &fs::Metadata) -> Option<i64> {
    let t = meta.modified().ok()?;
    let d = t.duration_since(std::time::UNIX_EPOCH).ok()?;
    i64::try_from(d.as_secs()).ok()
}

/// When a file was moved into a trash folder: its status-change time, which
/// a move updates and a later read does not.
fn moved_time(meta: &fs::Metadata) -> Option<i64> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some(meta.ctime())
            .filter(|t| *t > 0)
            .or_else(|| mtime(meta))
    }
    #[cfg(not(unix))]
    {
        mtime(meta)
    }
}

fn is_sqlite(path: &Path) -> bool {
    let mut head = [0u8; 16];
    fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut head))
        .map(|_| head == *sqlite::HEADER)
        .unwrap_or(false)
}

fn base_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

impl Scanner<'_> {
    pub fn scan(&mut self, folder: &Path) -> Report {
        let mut report = Report {
            root: folder.display().to_string(),
            ..Report::default()
        };
        if let Some(root) = ios::backup_root(folder) {
            report.root = root.display().to_string();
            self.scan_ios(&root, &mut report);
        } else {
            self.scan_tree(folder, &mut report);
        }
        if report.findings.len() >= MAX_FINDINGS {
            report.note(format!(
                "Stopped listing after {MAX_FINDINGS} findings. Narrow the scan to one folder to see the rest."
            ));
        }
        if self.cancel.load(Ordering::Relaxed) {
            report.note("The scan was stopped before it finished; these are the findings so far.");
        }
        (self.progress)(&Progress {
            phase: "Done".into(),
            files: report.files_checked,
            findings: report.findings.len(),
            current: String::new(),
        });
        report
    }

    fn tick(&mut self, report: &Report, phase: &str, current: &str) {
        if report.files_checked % 250 == 0 {
            (self.progress)(&Progress {
                phase: phase.into(),
                files: report.files_checked,
                findings: report.findings.len(),
                current: current.into(),
            });
        }
    }

    fn stop(&self, report: &Report) -> bool {
        self.cancel.load(Ordering::Relaxed) || report.findings.len() >= MAX_FINDINGS
    }

    fn scan_tree(&mut self, root: &Path, report: &mut Report) {
        let mut stack = vec![root.to_path_buf()];
        let mut photo_libraries = Vec::new();
        while let Some(dir) = stack.pop() {
            if self.stop(report) {
                return;
            }
            let Ok(entries) = fs::read_dir(&dir) else {
                if dir == root {
                    report.note(format!(
                        "Banker could not open {}. On a Mac, allow Full Disk Access in System Settings > Privacy & Security.",
                        root.display()
                    ));
                }
                continue;
            };
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if path == self.out {
                    continue;
                }
                let Ok(meta) = fs::symlink_metadata(&path) else {
                    continue;
                };
                let name = entry.file_name().to_string_lossy().into_owned();
                if meta.is_dir() {
                    if !SKIP_DIRS.contains(&name.as_str()) {
                        if name.ends_with(".photoslibrary") {
                            photo_libraries.push(path.clone());
                        }
                        stack.push(path);
                    }
                    continue;
                }
                if !meta.is_file() {
                    continue;
                }
                report.files_checked += 1;
                let rel = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                self.tick(report, "Checking files", &rel);
                self.examine(report, &path, &rel, &meta, None);
            }
        }
        for library in photo_libraries {
            let db = library.join("database").join("Photos.sqlite");
            let resolve = |dir: &str, file: &str| {
                ["originals", "Masters"]
                    .iter()
                    .map(|top| library.join(top).join(dir).join(file))
                    .find(|p| p.is_file())
            };
            self.photos_library(report, &db, None, &resolve);
        }
    }

    /// Look at one file: trash, damage, and deleted records if it is a database.
    fn examine(
        &mut self,
        report: &mut Report,
        path: &Path,
        rel: &str,
        meta: &fs::Metadata,
        ios: Option<IosContext>,
    ) {
        let device_name = base_name(rel);
        let app = match ios {
            Some((e, _, _)) => app_for_ios(&e.domain, &e.relative_path),
            None => app_for_path(rel),
        };
        let file_time = ios.and_then(|(e, _, _)| e.modified).or_else(|| mtime(meta));
        let size = ios.and_then(|(e, _, _)| e.size).unwrap_or(meta.len());
        let damage = integrity::check(path, device_name);

        if let Some(t) = trashed(rel) {
            let mut f = Finding::new(
                Status::Deleted,
                Source::TrashFolder,
                media_for_name(&t.name),
                t.name.clone(),
            );
            f.reason = match &damage {
                Some(d) => format!("{}. Also damaged: {d}", t.reason),
                None => t.reason.to_string(),
            };
            f.path = Some(path.display().to_string());
            f.original_path = Some(rel.to_string());
            f.app = app;
            f.size = size;
            f.time = t.deleted_at.or_else(|| moved_time(meta));
            report.push(f);
            return;
        }

        if let Some(reason) = damage {
            let mut f = Finding::new(
                Status::Corrupt,
                Source::DamagedFile,
                media_for_name(device_name),
                device_name.to_string(),
            );
            f.reason = reason;
            f.path = Some(path.display().to_string());
            f.original_path = Some(rel.to_string());
            f.app = app.clone();
            f.size = size;
            f.time = file_time;
            report.push(f);
        }

        let journal = rel.ends_with("-wal") || rel.ends_with("-shm") || rel.ends_with("-journal");
        if journal || !is_sqlite(path) {
            return;
        }
        if meta.len() > MAX_DB_BYTES {
            report.note(format!(
                "Skipped {device_name}: databases over {} MB are not examined yet.",
                MAX_DB_BYTES / 1024 / 1024
            ));
            return;
        }
        let wal: Option<PathBuf> = match ios {
            Some((e, by_path, root)) => by_path
                .get(&(e.domain.clone(), format!("{}-wal", e.relative_path)))
                .map(|w| w.stored_at(root)),
            None => Some(PathBuf::from(format!("{}-wal", path.display()))),
        };
        let ctx = DbContext {
            name: device_name,
            app,
            path,
            original: Some(rel.to_string()),
        };
        self.database(report, &ctx, wal.as_deref());
    }

    fn database(&mut self, report: &mut Report, ctx: &DbContext, wal: Option<&Path>) {
        let Ok(data) = fs::read(ctx.path) else { return };
        let wal_bytes = wal.and_then(|w| fs::read(w).ok());
        report.databases_checked += 1;
        let Ok(recovery) = sqlite::recover(data, wal_bytes.as_deref(), self.now) else {
            return;
        };
        for frag in recovery.fragments {
            let mut f = Finding::new(
                Status::Deleted,
                frag.source,
                Media::Text,
                ctx.name.to_string(),
            );
            let table = frag
                .table
                .map(|t| format!("the {t} table in "))
                .unwrap_or_default();
            f.reason = match frag.source {
                Source::DatabaseHistory => {
                    format!(
                        "An earlier version, kept in the journal of {table}{}",
                        ctx.name
                    )
                }
                _ => format!("Deleted from {table}{}", ctx.name),
            };
            f.path = Some(ctx.path.display().to_string());
            f.original_path = ctx.original.clone();
            f.app = ctx.app.clone();
            f.size = frag.text.len() as u64;
            f.time = frag.time;
            f.time_estimated = frag.time_estimated;
            f.text = Some(frag.text);
            report.push(f);
        }
        if recovery.pictures.is_empty() {
            return;
        }
        let dir = self.out.join("rebuilt");
        if fs::create_dir_all(&dir).is_err() {
            report.note("Pictures were found in deleted space, but could not be saved.");
            return;
        }
        for pic in recovery.pictures {
            let file = dir.join(format!(
                "{}-{}.{}",
                report.findings.len() + 1,
                ctx.name.replace(['/', '.'], "_"),
                pic.kind.extension()
            ));
            if fs::write(&file, &pic.bytes).is_err() {
                continue;
            }
            let mut f = Finding::new(
                Status::Deleted,
                Source::CarvedFromDatabase,
                Media::Image,
                file.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            );
            f.reason = format!("Picture rebuilt from deleted space in {}", ctx.name);
            f.path = Some(file.display().to_string());
            f.original_path = ctx.original.clone();
            f.app = ctx.app.clone();
            f.size = pic.bytes.len() as u64;
            report.push(f);
        }
    }

    /// "Recently Deleted" photos and videos in a Photos library database.
    fn photos_library(
        &mut self,
        report: &mut Report,
        db_path: &Path,
        wal: Option<&Path>,
        resolve: &dyn Fn(&str, &str) -> Option<PathBuf>,
    ) {
        let Ok(data) = fs::read(db_path) else { return };
        let Ok(mut db) = Db::parse(data) else { return };
        if let Some(w) = wal.and_then(|w| fs::read(w).ok()) {
            db.apply_wal(&w);
        }
        for asset in ios::trashed_assets(&db) {
            let file = resolve(&asset.directory, &asset.filename);
            let media = if asset.video {
                Media::Video
            } else {
                Media::Image
            };
            let mut f = Finding::new(
                Status::Deleted,
                Source::PhotosRecentlyDeleted,
                media,
                asset.filename.clone(),
            );
            f.reason = if file.is_some() {
                "In Photos > Recently Deleted".into()
            } else {
                "In Photos > Recently Deleted. Only its record is left; the file itself is gone"
                    .into()
            };
            f.size = file
                .as_ref()
                .and_then(|p| fs::metadata(p).ok())
                .map_or(0, |m| m.len());
            f.path = file.map(|p| p.display().to_string());
            f.original_path = Some(format!("{}/{}", asset.directory, asset.filename));
            f.app = Some("Photos".into());
            f.time = asset.trashed_at.or(asset.created_at);
            report.push(f);
        }
    }

    fn scan_ios(&mut self, root: &Path, report: &mut Report) {
        let entries = match ios::read_manifest(root) {
            Manifest::Entries(e) => e,
            Manifest::Encrypted => {
                report.note(
                    "This iPhone backup is encrypted, so its contents cannot be read. In Finder, select the iPhone and turn off \"Encrypt local backup\", then back up again.",
                );
                return;
            }
            Manifest::Unreadable => {
                report.note("The iPhone backup is incomplete: Manifest.db could not be read.");
                return;
            }
        };
        let by_path: HashMap<(String, String), ios::Entry> = entries
            .iter()
            .map(|e| ((e.domain.clone(), e.relative_path.clone()), e.clone()))
            .collect();
        for entry in &entries {
            if self.stop(report) {
                return;
            }
            let stored = entry.stored_at(root);
            let Ok(meta) = fs::metadata(&stored) else {
                continue;
            };
            report.files_checked += 1;
            let rel = entry.relative_path.as_str();
            self.tick(report, "Checking the iPhone backup", rel);
            self.examine(report, &stored, rel, &meta, Some((entry, &by_path, root)));
        }
        let photos = (
            "CameraRollDomain".to_string(),
            "Media/PhotoData/Photos.sqlite".to_string(),
        );
        if let Some(db) = by_path.get(&photos) {
            let wal = by_path
                .get(&(photos.0.clone(), format!("{}-wal", photos.1)))
                .map(|w| w.stored_at(root));
            let resolve = |dir: &str, file: &str| {
                by_path
                    .get(&(
                        "CameraRollDomain".to_string(),
                        format!("Media/{dir}/{file}"),
                    ))
                    .map(|e| e.stored_at(root))
                    .filter(|p| p.is_file())
            };
            self.photos_library(report, &db.stored_at(root), wal.as_deref(), &resolve);
        }
    }
}

/// Everything one press of Scan does: copy a phone to the Mac when needed
/// (into `~/Banker Backups/...`), then scan the copy or the drive.
pub fn scan_device(
    device: &Device,
    home: &Path,
    now: i64,
    tz_offset_minutes: i32,
    work: &Path,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(&Progress),
) -> Result<Report, String> {
    let copies = matches!(device.kind, DeviceKind::Iphone | DeviceKind::Android);
    let folder = if copies {
        let dest = backup_folder(home, &device.name, now + tz_offset_minutes as i64 * 60);
        progress(&Progress {
            phase: match device.kind {
                DeviceKind::Iphone => "Backing up the iPhone to this Mac (keep it unlocked)",
                _ => "Copying the phone to this Mac",
            }
            .into(),
            current: dest.display().to_string(),
            ..Progress::default()
        });
        copy_to_mac(device, &dest, cancel)?
    } else {
        copy_to_mac(device, Path::new(""), cancel)?
    };
    let (folder, warning) = folder;
    let mut scanner = Scanner {
        now,
        out: work.join("recovered"),
        cancel,
        progress,
    };
    let mut report = scanner.scan(&folder);
    if let Some(w) = warning {
        report.note(w);
    }
    if copies {
        report.note(format!(
            "A copy of the phone was saved in {}.",
            folder.display()
        ));
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrity::tests::{temp_dir, tiny_jpeg};
    use crate::sqlite::tests::tiny_db;

    fn run(dir: &Path) -> Report {
        let cancel = AtomicBool::new(false);
        let mut seen = 0;
        let mut progress = |_: &Progress| seen += 1;
        let mut scanner = Scanner {
            now: 1_760_000_000,
            out: dir.join("out"),
            cancel: &cancel,
            progress: &mut progress,
        };
        scanner.scan(dir)
    }

    #[test]
    fn finds_trash_damage_and_deleted_records_only() {
        let dir = temp_dir("scan");
        fs::create_dir_all(dir.join(".Trashes/501")).unwrap();
        fs::create_dir_all(dir.join("DCIM/Camera")).unwrap();
        fs::create_dir_all(dir.join("Android/data/com.example.chat/databases")).unwrap();
        let j = tiny_jpeg();
        fs::write(dir.join(".Trashes/501/gone.jpg"), &j).unwrap();
        fs::write(dir.join("DCIM/Camera/fine.jpg"), &j).unwrap();
        fs::write(dir.join("DCIM/Camera/broken.jpg"), &j[..j.len() - 3]).unwrap();
        fs::write(dir.join("DCIM/Camera/.trashed-1712592000-IMG_1.jpg"), &j).unwrap();
        fs::write(
            dir.join("Android/data/com.example.chat/databases/chat.db"),
            tiny_db(),
        )
        .unwrap();

        let report = run(&dir);
        let names: Vec<_> = report.findings.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"gone.jpg"), "{names:?}");
        assert!(names.contains(&"IMG_1.jpg"), "{names:?}");
        assert!(names.contains(&"broken.jpg"), "{names:?}");
        assert!(
            !names.contains(&"fine.jpg"),
            "healthy files are never listed"
        );

        let broken = report
            .findings
            .iter()
            .find(|f| f.name == "broken.jpg")
            .unwrap();
        assert_eq!(broken.status, Status::Corrupt);
        assert_eq!(broken.app.as_deref(), Some("Camera"));

        let msg = report
            .findings
            .iter()
            .find(|f| f.text.as_deref() == Some("secret plans for friday"))
            .expect("deleted row");
        assert_eq!(msg.app.as_deref(), Some("com.example.chat"));
        assert!(msg.reason.contains("msg table"));
        assert!(!report.findings.iter().any(|f| f
            .text
            .as_deref()
            .is_some_and(|t| t.contains("kept message"))));
        assert_eq!(report.databases_checked, 1);
    }

    /// The folder `adb pull -a /sdcard/ <dest>` leaves behind.
    #[test]
    fn scans_an_android_copy() {
        let dest = temp_dir("android");
        let sd = dest.join("sdcard");
        let j = tiny_jpeg();
        let files: &[(&str, &[u8])] = &[
            ("DCIM/Camera/IMG_20250301_101500.jpg", &j),
            (
                "DCIM/Camera/.trashed-1745000000-IMG_20250302_120000.jpg",
                &j,
            ),
            (
                "Download/.trashed-1745100000-boarding pass.pdf",
                b"%PDF-1.4\n%%EOF",
            ),
            (
                "Android/.Trash/com.sec.android.gallery3d/1700000000000/IMG_1.jpg",
                &j,
            ),
            (
                "Android/data/com.sec.android.gallery3d/files/.Trash/IMG_2.jpg",
                &j,
            ),
            (
                "Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Images/IMG-WA0001.jpg",
                &j[..j.len() - 3],
            ),
            ("LOST.DIR/1234", b"leftover"),
        ];
        for (rel, data) in files {
            let path = sd.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, data).unwrap();
        }
        let db = sd.join("Android/data/com.example.notes/files/notes.db");
        fs::create_dir_all(db.parent().unwrap()).unwrap();
        fs::write(&db, tiny_db()).unwrap();
        fs::write(dest.join("banker-copy-log.txt"), "pulled").unwrap();

        let report = run(&dest);
        let find = |name: &str| {
            report
                .findings
                .iter()
                .find(|f| f.name == name)
                .unwrap_or_else(|| panic!("{name} missing: {:?}", report.findings))
        };

        let trashed = find("IMG_20250302_120000.jpg");
        assert_eq!(trashed.status, Status::Deleted);
        assert_eq!(trashed.app.as_deref(), Some("Camera"));
        assert_eq!(trashed.time, Some(1_745_000_000 - 30 * 86_400));
        assert_eq!(find("boarding pass.pdf").status, Status::Deleted);
        assert_eq!(
            find("IMG_1.jpg").app.as_deref(),
            Some("com.sec.android.gallery3d")
        );
        assert_eq!(
            find("IMG_2.jpg").app.as_deref(),
            Some("com.sec.android.gallery3d")
        );
        let broken = find("IMG-WA0001.jpg");
        assert_eq!(broken.status, Status::Corrupt);
        assert_eq!(broken.app.as_deref(), Some("com.whatsapp"));
        assert_eq!(find("1234").status, Status::Deleted);
        let note = report
            .findings
            .iter()
            .find(|f| f.text.as_deref() == Some("secret plans for friday"))
            .expect("deleted row in an app database");
        assert_eq!(note.app.as_deref(), Some("com.example.notes"));

        assert!(
            !report
                .findings
                .iter()
                .any(|f| f.name == "IMG_20250301_101500.jpg" || f.name.contains("banker-copy-log")),
            "live files are never listed"
        );
    }

    #[test]
    fn stops_when_cancelled() {
        let dir = temp_dir("cancel");
        fs::write(dir.join("a.jpg"), b"").unwrap();
        let cancel = AtomicBool::new(true);
        let mut progress = |_: &Progress| {};
        let mut scanner = Scanner {
            now: 0,
            out: dir.join("out"),
            cancel: &cancel,
            progress: &mut progress,
        };
        let report = scanner.scan(&dir);
        assert!(report.findings.is_empty());
        assert!(report.notes.iter().any(|n| n.contains("stopped")));
    }
}
