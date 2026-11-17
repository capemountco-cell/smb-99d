//! Scan a folder from the terminal and print what Banker finds.
//!
//!     cargo run -p banker-core --example scan -- /Volumes/Card

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use banker_core::scan::{Progress, Scanner};

fn main() {
    let Some(folder) = std::env::args().nth(1).map(PathBuf::from) else {
        eprintln!("usage: scan <folder>");
        std::process::exit(2);
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let cancel = AtomicBool::new(false);
    let mut progress = |_: &Progress| {};
    let mut scanner = Scanner {
        now,
        out: std::env::temp_dir().join("banker-example-out"),
        cancel: &cancel,
        progress: &mut progress,
    };
    let report = scanner.scan(&folder);
    for f in &report.findings {
        println!(
            "{:?}\t{:?}\t{}\t{}\t{}\t{:?}",
            f.status,
            f.source,
            f.app.as_deref().unwrap_or("-"),
            f.name,
            f.text.as_deref().unwrap_or(&f.reason),
            f.time
        );
    }
    eprintln!(
        "{} files, {} databases, {} findings",
        report.files_checked,
        report.databases_checked,
        report.findings.len()
    );
    for note in &report.notes {
        eprintln!("note: {note}");
    }
}
