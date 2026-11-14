//! A Mac Photos library, as "Optimize Mac Storage" leaves it: some originals
//! are only in iCloud, and Photos keeps a full-size preview instead. The
//! database is real SQLite; see fixtures/README.md.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use banker_core::scan::{Progress, Scanner};
use banker_core::{Media, Source, Status};

const ICLOUD_ONLY: &str = "0A1B2C3D-0000-4000-8000-000000000002";
const LOCAL: &str = "0A1B2C3D-0000-4000-8000-000000000001";

fn jpeg() -> Vec<u8> {
    let mut j = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x06, b'J', b'F', b'I', b'F'];
    j.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x02, 0x12, 0x34, 0x56, 0xFF, 0xD9]);
    j
}

fn home() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("reco-mac-photos-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let library = dir.join("Pictures/Photos Library.photoslibrary");
    fs::create_dir_all(library.join("database")).unwrap();
    fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/mac-photos.sqlite"
        ),
        library.join("database/Photos.sqlite"),
    )
    .unwrap();
    fs::create_dir_all(library.join("originals/0")).unwrap();
    fs::write(library.join(format!("originals/0/{LOCAL}.jpeg")), jpeg()).unwrap();
    // The iCloud-only photo: no original, two previews; the larger one wins.
    let previews = library.join("resources/derivatives/0");
    fs::create_dir_all(&previews).unwrap();
    fs::write(
        previews.join(format!("{ICLOUD_ONLY}_1_105_c.jpeg")),
        jpeg().repeat(3),
    )
    .unwrap();
    fs::write(
        previews.join(format!("{ICLOUD_ONLY}_4_5005_c.jpeg")),
        jpeg(),
    )
    .unwrap();
    // A file that was emptied: shown as damaged, never as something to recover.
    fs::write(dir.join("Pictures/IMG_6357.PNG"), b"").unwrap();
    dir
}

#[test]
fn deleted_photos_come_back_with_their_real_names_even_when_in_icloud() {
    let home = home();
    let cancel = AtomicBool::new(false);
    let mut progress = |_: &Progress| {};
    let mut scanner = Scanner {
        now: 1_800_000_000,
        out: home.join("out"),
        cancel: &cancel,
        progress: &mut progress,
    };
    let report = scanner.scan_mac(&home);
    let photo = |name: &str| {
        report
            .findings
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("{name} missing: {:#?}", report.findings))
    };

    let local = photo("IMG_6352.JPG");
    assert_eq!(local.source, Source::PhotosRecentlyDeleted);
    assert_eq!(local.media, Media::Image);
    assert!(local.path.as_deref().unwrap().contains("originals"));
    assert!(banker_core::export::recoverable(local));

    let cloud = photo("IMG_6353.HEIC");
    assert!(cloud.path.as_deref().unwrap().ends_with("_1_105_c.jpeg"));
    assert!(cloud.reason.contains("iCloud"));
    assert!(cloud.size > 0);
    assert!(banker_core::export::recoverable(cloud));

    assert!(
        !report.findings.iter().any(|f| f.name == "IMG_6354.HEIC"),
        "a photo that was not deleted is never listed"
    );

    let empty = photo("IMG_6357.PNG");
    assert_eq!(empty.status, Status::Corrupt);
    assert!(!banker_core::export::recoverable(empty));
}
