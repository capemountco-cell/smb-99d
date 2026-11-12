//! Where Banker keeps what it copies: scan copies, clones and saved items.
//!
//! By default that is `~/Banker Backups`. The person can choose any folder
//! instead, such as an external drive; Banker then uses `<that folder>/Banker
//! Backups`. The choice lives in
//! `~/Library/Application Support/Banker/settings.json`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

pub const FOLDER: &str = "Banker Backups";

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// The folder the person chose. None: their home folder.
    pub save_to: Option<String>,
}

/// The current "Save to" location, for the UI.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    /// Where things are saved: `<chosen folder>/Banker Backups`.
    pub path: String,
    /// A short name: the drive's name, or "This Mac".
    pub label: String,
    pub free_bytes: Option<u64>,
    /// False when the chosen drive is not plugged in.
    pub available: bool,
    pub is_default: bool,
}

fn settings_path(home: &Path) -> PathBuf {
    home.join("Library/Application Support/Banker/settings.json")
}

pub fn load(home: &Path) -> Settings {
    fs::read_to_string(settings_path(home))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save(home: &Path, settings: &Settings) -> Result<(), String> {
    let path = settings_path(home);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| format!("Could not save the setting: {e}"))
}

fn default_root(home: &Path) -> PathBuf {
    home.join(FOLDER)
}

fn chosen_root(home: &Path) -> Option<PathBuf> {
    load(home).save_to.map(|p| PathBuf::from(p).join(FOLDER))
}

/// The folder new copies go into. An error when the chosen drive is not
/// connected, so nothing is quietly written to the Mac's own disk instead.
pub fn save_root(home: &Path) -> Result<PathBuf, String> {
    match load(home).save_to {
        None => Ok(default_root(home)),
        Some(chosen) if Path::new(&chosen).is_dir() => Ok(PathBuf::from(chosen).join(FOLDER)),
        Some(chosen) => Err(format!(
            "The Save to location, {chosen}, is not connected. Plug in the drive, or choose another location under Backups."
        )),
    }
}

/// Every folder Banker may have saved into: the default and the chosen one.
/// Used to list clones and to accept a clone folder the window names.
pub fn known_roots(home: &Path) -> Vec<PathBuf> {
    let mut roots = vec![default_root(home)];
    if let Some(chosen) = chosen_root(home) {
        if !roots.contains(&chosen) {
            roots.push(chosen);
        }
    }
    roots
}

/// The drive a path is on, as the person knows it.
fn label_for(path: &Path) -> String {
    let mut parts = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned());
    match (parts.next(), parts.next(), parts.next()) {
        (Some(root), Some(volumes), Some(name)) if root == "/" && volumes == "Volumes" => name,
        _ => "This Mac".into(),
    }
}

pub fn location(home: &Path) -> Location {
    let chosen = load(home).save_to;
    let base = chosen
        .clone()
        .map(PathBuf::from)
        .unwrap_or_else(|| home.to_path_buf());
    let available = base.is_dir();
    Location {
        path: base.join(FOLDER).display().to_string(),
        label: label_for(&base),
        free_bytes: if available { free_space(&base) } else { None },
        available,
        is_default: chosen.is_none(),
    }
}

/// Choose where to save, or None for the default. The folder must exist and
/// be writable.
pub fn set_save_to(home: &Path, folder: Option<&str>) -> Result<Location, String> {
    if let Some(folder) = folder {
        let dir = Path::new(folder);
        if !dir.is_dir() {
            return Err(format!("{folder} is not a folder."));
        }
        let probe = dir.join(FOLDER).join(".banker-write-test");
        fs::create_dir_all(dir.join(FOLDER))
            .and_then(|_| fs::write(&probe, b"ok"))
            .map_err(|e| format!("Banker cannot write to {folder}: {e}"))?;
        let _ = fs::remove_file(probe);
    }
    save(
        home,
        &Settings {
            save_to: folder.map(String::from),
        },
    )?;
    Ok(location(home))
}

/// Available bytes from `df -k` output (the last line, fourth column).
pub fn parse_df_available(df: &str) -> Option<u64> {
    let line = df.lines().rev().find(|l| !l.trim().is_empty())?;
    let kb: u64 = line.split_whitespace().nth(3)?.parse().ok()?;
    Some(kb * 1024)
}

/// Free space where `path` lives.
pub fn free_space(path: &Path) -> Option<u64> {
    let out = Command::new("/bin/df")
        .arg("-k")
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    parse_df_available(&String::from_utf8_lossy(&out.stdout))
}

/// Size of everything under `dir`.
pub fn folder_size(dir: &Path) -> u64 {
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

/// Refuse to start a copy that cannot fit, in words the person can act on.
/// `needed` is what the copy will add; `already` is what an earlier copy in
/// the same place already holds (an iPhone backup only adds what changed).
pub fn check_space(
    root: &Path,
    what: &str,
    needed: Option<u64>,
    already: u64,
) -> Result<(), String> {
    let Some(needed) = needed else { return Ok(()) };
    let base = root.parent().unwrap_or(root);
    let Some(free) = free_space(base) else {
        return Ok(());
    };
    let adds = needed.saturating_sub(already);
    // A margin for the backup's own index and the Mac's breathing room.
    let wanted = adds + adds / 10 + 1_000_000_000;
    if free >= wanted {
        return Ok(());
    }
    Err(format!(
        "Not enough space on {}: {what} needs about {}, and only {} is free. Choose another Save to location (an external drive works) under Backups, or free up space.",
        label_for(base),
        human(wanted),
        human(free)
    ))
}

pub fn human(bytes: u64) -> String {
    let gb = bytes as f64 / 1e9;
    if gb >= 10.0 {
        format!("{gb:.0} GB")
    } else if gb >= 1.0 {
        format!("{gb:.1} GB")
    } else {
        format!("{:.0} MB", bytes as f64 / 1e6)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrity::tests::temp_dir;

    #[test]
    fn reads_df() {
        let df = "Filesystem   1024-blocks      Used Available Capacity iused ifree %iused  Mounted on\n\
                  /dev/disk3s5   482797652 301234567 160000000    66% 1234 5678 1%   /System/Volumes/Data\n";
        assert_eq!(parse_df_available(df), Some(160_000_000 * 1024));
        assert_eq!(parse_df_available("nonsense"), None);
    }

    #[test]
    fn default_and_chosen_locations() {
        let home = temp_dir("settings");
        assert_eq!(save_root(&home).unwrap(), home.join(FOLDER));
        assert!(location(&home).is_default);

        let drive = home.join("Volumes/T7");
        fs::create_dir_all(&drive).unwrap();
        let loc = set_save_to(&home, Some(&drive.display().to_string())).unwrap();
        assert!(!loc.is_default);
        assert_eq!(save_root(&home).unwrap(), drive.join(FOLDER));
        assert_eq!(
            known_roots(&home),
            vec![home.join(FOLDER), drive.join(FOLDER)]
        );

        // The drive is unplugged: never fall back to the Mac's disk silently.
        fs::remove_dir_all(&drive).unwrap();
        assert!(save_root(&home).unwrap_err().contains("not connected"));
        assert!(!location(&home).available);

        set_save_to(&home, None).unwrap();
        assert_eq!(save_root(&home).unwrap(), home.join(FOLDER));
        assert!(set_save_to(&home, Some("/no/such/folder")).is_err());
    }

    #[test]
    fn labels_and_sizes() {
        assert_eq!(label_for(Path::new("/Volumes/Samsung T7/x")), "Samsung T7");
        assert_eq!(label_for(Path::new("/Users/me")), "This Mac");
        assert_eq!(human(57_300_000_000), "57 GB");
        assert_eq!(human(1_500_000_000), "1.5 GB");
        assert_eq!(human(300_000_000), "300 MB");
    }
}
