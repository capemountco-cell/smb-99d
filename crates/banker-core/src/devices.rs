//! Finding connected devices and copying them to the Mac.
//!
//! * Drives (USB disks, SD cards, cameras) are mounted under /Volumes and
//!   are scanned in place, read-only.
//! * iPhones are backed up with `idevicebackup2` (libimobiledevice), then the
//!   backup is scanned.
//! * Android phones are copied with `adb pull`, then the copy is scanned.
//! * Existing Finder backups of iPhones can be scanned directly.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::timeline::civil_from_days;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DeviceKind {
    Drive,
    Iphone,
    Android,
    /// An iPhone backup already on this Mac.
    Backup,
    /// Any folder the user names.
    Folder,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    /// Volume path, UDID or adb serial.
    pub id: String,
    pub kind: DeviceKind,
    pub name: String,
    pub detail: String,
    /// The folder to scan, for drives, backups and folders.
    pub path: Option<String>,
    /// Why it cannot be scanned yet, in words the user can act on.
    pub problem: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolHint {
    /// What to do, as one sentence.
    pub text: String,
    /// A Terminal command that does it, when there is one.
    pub command: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Discovery {
    pub devices: Vec<Device>,
    pub hints: Vec<ToolHint>,
}

/// `adb devices -l` output: (serial, state, model).
pub fn parse_adb_devices(out: &str) -> Vec<(String, String, Option<String>)> {
    out.lines()
        .skip_while(|l| !l.starts_with("List of devices"))
        .skip(1)
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let serial = parts.next()?.to_string();
            let state = parts.next()?.to_string();
            let model = parts
                .find_map(|p| p.strip_prefix("model:"))
                .map(|m| m.replace('_', " "));
            Some((serial, state, model))
        })
        .collect()
}

/// `idevice_id -l` output: one UDID per line.
pub fn parse_idevice_ids(out: &str) -> Vec<String> {
    out.lines()
        .map(str::trim)
        .filter(|l| l.len() >= 24 && l.chars().all(|c| c.is_ascii_hexdigit() || c == '-'))
        .map(String::from)
        .collect()
}

/// Value of `<key>name</key><string>…</string>` in an XML plist.
pub fn xml_plist_string(xml: &str, key: &str) -> Option<String> {
    let at = xml.find(&format!("<key>{key}</key>"))?;
    let rest = &xml[at..];
    let open = rest.find("<string>")? + "<string>".len();
    let close = rest[open..].find("</string>")?;
    Some(rest[open..open + close].replace("&amp;", "&"))
}

/// Mounted drives under `volumes`, leaving out the Mac's own disk.
pub fn drives(volumes: &Path) -> Vec<Device> {
    let Ok(entries) = std::fs::read_dir(volumes) else {
        return Vec::new();
    };
    let mut out: Vec<Device> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let path = e.path();
            let real = std::fs::canonicalize(&path).ok()?;
            if real == Path::new("/") || !real.is_dir() {
                return None;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name == "com.apple.TimeMachine.localsnapshots" {
                return None;
            }
            Some(Device {
                id: path.display().to_string(),
                kind: DeviceKind::Drive,
                name,
                detail: "Drive · scanned in place, nothing is changed".into(),
                path: Some(path.display().to_string()),
                problem: None,
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// iPhone backups made by Finder, in `~/Library/Application Support/MobileSync/Backup`.
pub fn finder_backups(dir: &Path) -> Vec<Device> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|e| e.path().join("Manifest.db").is_file())
        .map(|e| {
            let path = e.path();
            let info = std::fs::read_to_string(path.join("Info.plist")).unwrap_or_default();
            let name = xml_plist_string(&info, "Device Name").unwrap_or_else(|| "iPhone".into());
            let product = xml_plist_string(&info, "Product Name").unwrap_or_default();
            Device {
                id: path.display().to_string(),
                kind: DeviceKind::Backup,
                name,
                detail: format!("{product} backup on this Mac").trim().to_string(),
                path: Some(path.display().to_string()),
                problem: None,
            }
        })
        .collect()
}

/// A command-line tool, looked up where Homebrew installs it as well as on
/// PATH: apps opened from Finder do not inherit the shell's PATH.
pub fn find_tool(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .chain(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from))
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

fn output(tool: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new(tool)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr))
}

/// Everything connected or available right now.
pub fn discover(home: &Path) -> Discovery {
    let mut d = Discovery::default();
    d.devices.extend(drives(Path::new("/Volumes")));

    match find_tool("idevice_id") {
        Some(tool) => {
            let info = find_tool("ideviceinfo");
            for udid in parse_idevice_ids(&output(&tool, &["-l"]).unwrap_or_default()) {
                let name = info
                    .as_ref()
                    .and_then(|i| output(i, &["-u", &udid, "-k", "DeviceName"]))
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty() && !s.contains("ERROR"))
                    .unwrap_or_else(|| "iPhone".into());
                d.devices.push(Device {
                    id: udid,
                    kind: DeviceKind::Iphone,
                    name,
                    detail: "iPhone · backed up to this Mac, then scanned".into(),
                    path: None,
                    problem: None,
                });
            }
        }
        None => d.hints.push(ToolHint {
            text: "To back up iPhones and iPads over USB, install libimobiledevice:".into(),
            command: Some("brew install libimobiledevice".into()),
        }),
    }

    match find_tool("adb") {
        Some(tool) => {
            for (serial, state, model) in
                parse_adb_devices(&output(&tool, &["devices", "-l"]).unwrap_or_default())
            {
                let problem = match state.as_str() {
                    "device" => None,
                    "unauthorized" => Some(
                        "On the phone, tap Allow when asked to allow USB debugging from this Mac."
                            .into(),
                    ),
                    other => Some(format!(
                        "The phone reports \"{other}\". Unplug it and plug it back in."
                    )),
                };
                d.devices.push(Device {
                    id: serial,
                    kind: DeviceKind::Android,
                    name: model.unwrap_or_else(|| "Android phone".into()),
                    detail: "Android · copied to this Mac, then scanned".into(),
                    path: None,
                    problem,
                });
            }
            if !d.devices.iter().any(|x| x.kind == DeviceKind::Android) {
                d.hints.push(ToolHint {
                    text: "Android phone plugged in but not listed? Turn on USB debugging: Settings > About phone, tap Build number 7 times, then Settings > System > Developer options > USB debugging. Then tap Allow on the phone.".into(),
                    command: None,
                });
            }
        }
        None => d.hints.push(ToolHint {
            text: "To copy Android phones over USB, install Android platform tools, then turn on USB debugging on the phone:".into(),
            command: Some("brew install android-platform-tools".into()),
        }),
    }

    d.devices.extend(finder_backups(
        &home.join("Library/Application Support/MobileSync/Backup"),
    ));
    d
}

/// `~/Banker Backups/<name> <YYYY-MM-DD HH.MM>`
pub fn backup_folder(home: &Path, device_name: &str, now_local: i64) -> PathBuf {
    let (y, m, d) = civil_from_days(now_local.div_euclid(86_400));
    let secs = now_local.rem_euclid(86_400);
    let safe: String = device_name
        .chars()
        .map(|c| {
            if matches!(c, '/' | ':' | '\\') {
                '-'
            } else {
                c
            }
        })
        .collect();
    home.join("Banker Backups").join(format!(
        "{safe} {y}-{m:02}-{d:02} {:02}.{:02}",
        secs / 3600,
        secs % 3600 / 60
    ))
}

/// Run a tool until it finishes or the user cancels. Its output goes to a
/// log file in `dest`, so a chatty tool can never fill a pipe and stall.
fn run(tool: &Path, args: &[&str], dest: &Path, cancel: &AtomicBool) -> Result<String, String> {
    std::fs::create_dir_all(dest)
        .map_err(|e| format!("Could not create {}: {e}", dest.display()))?;
    let log_path = dest.join("banker-copy-log.txt");
    let log = std::fs::File::create(&log_path).map_err(|e| e.to_string())?;
    let err = log.try_clone().map_err(|e| e.to_string())?;
    let mut child = Command::new(tool)
        .args(args)
        .stdin(Stdio::null())
        .stdout(log)
        .stderr(err)
        .spawn()
        .map_err(|e| format!("Could not start {}: {e}", tool.display()))?;
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Stopped.".into());
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(250)),
            Err(e) => return Err(e.to_string()),
        }
    };
    let mut text = String::new();
    if let Ok(mut f) = std::fs::File::open(&log_path) {
        let _ = f.read_to_string(&mut text);
    }
    if status.success() {
        Ok(text)
    } else {
        let tail: Vec<&str> = text.lines().rev().take(3).collect();
        Err(tail.into_iter().rev().collect::<Vec<_>>().join(" "))
    }
}

/// The outcome of `adb pull`. On a real phone it nearly always exits with an
/// error because a few protected files cannot be read, even when everything
/// else copied. A copy with anything in it is kept and scanned, with a note;
/// only a copy that got nothing at all (or was stopped) is a failure.
pub fn android_pull_outcome(
    pulled: Result<String, String>,
    dest: &Path,
) -> Result<Option<String>, String> {
    let error = match pulled {
        Ok(_) => return Ok(None),
        Err(e) => e,
    };
    if error == "Stopped." {
        return Err(error);
    }
    let copied = std::fs::read_dir(dest.join("sdcard"))
        .map(|mut entries| entries.next().is_some())
        .unwrap_or(false);
    if copied {
        Ok(Some(
            "Some files on the phone could not be copied (Android protects them). Everything else was copied and scanned.".into(),
        ))
    } else {
        Err(format!(
            "Copying from the phone failed: {error} Unlock the phone, keep it unlocked, and check that USB debugging is still allowed."
        ))
    }
}

/// Copy a phone to `dest` and return the folder to scan, with a note when
/// part of the copy failed. Drives, backups and folders are scanned where
/// they are.
pub fn copy_to_mac(
    device: &Device,
    dest: &Path,
    cancel: &AtomicBool,
) -> Result<(PathBuf, Option<String>), String> {
    match device.kind {
        DeviceKind::Drive | DeviceKind::Backup | DeviceKind::Folder => device
            .path
            .as_ref()
            .map(|p| (PathBuf::from(p), None))
            .ok_or_else(|| "This device has no folder to scan.".into()),
        DeviceKind::Android => {
            let adb = find_tool("adb").ok_or("Install Android platform tools first.")?;
            let target = dest.display().to_string();
            // Pulls into <dest>/sdcard, keeping each file's original dates
            // (-a) so the timeline shows when things happened on the phone.
            let pulled = run(
                &adb,
                &["-s", &device.id, "pull", "-a", "/sdcard/", &target],
                dest,
                cancel,
            );
            let warning = android_pull_outcome(pulled, dest)?;
            Ok((dest.to_path_buf(), warning))
        }
        DeviceKind::Iphone => {
            let pair = find_tool("idevicepair").ok_or("Install libimobiledevice first.")?;
            let backup = find_tool("idevicebackup2").ok_or("Install libimobiledevice first.")?;
            if run(&pair, &["-u", &device.id, "validate"], dest, cancel).is_err() {
                let paired = run(&pair, &["-u", &device.id, "pair"], dest, cancel);
                if let Err(e) = paired {
                    return Err(if e.contains("trust") || e.contains("passcode") {
                        "On the iPhone, unlock it, tap Trust and enter the passcode, then press Scan again.".into()
                    } else {
                        format!("The iPhone did not pair with this Mac: {e}")
                    });
                }
            }
            let target = dest.display().to_string();
            run(
                &backup,
                &["-u", &device.id, "backup", "--full", &target],
                dest,
                cancel,
            )
            .map_err(|e| format!("Backing up the iPhone failed: {e}"))?;
            Ok((dest.join(&device.id), None))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrity::tests::temp_dir;

    #[test]
    fn parses_adb() {
        let out = "* daemon started successfully\nList of devices attached\n\
                   R58M123ABC  device usb:1-1 product:beyond1 model:SM_G973F device:beyond1\n\
                   emulator-5554 unauthorized\n\n";
        assert_eq!(
            parse_adb_devices(out),
            vec![
                (
                    "R58M123ABC".into(),
                    "device".into(),
                    Some("SM G973F".into())
                ),
                ("emulator-5554".into(), "unauthorized".into(), None),
            ]
        );
    }

    #[test]
    fn parses_udids() {
        let out = "00008030-001A2B3C4D5E802E\nNo device found\n";
        assert_eq!(parse_idevice_ids(out), vec!["00008030-001A2B3C4D5E802E"]);
    }

    #[test]
    fn reads_xml_plist_strings() {
        let xml = "<dict><key>Device Name</key>\n<string>Sam &amp; Co</string></dict>";
        assert_eq!(
            xml_plist_string(xml, "Device Name").as_deref(),
            Some("Sam & Co")
        );
        assert_eq!(xml_plist_string(xml, "Missing"), None);
    }

    #[test]
    fn lists_drives_but_not_the_system_disk() {
        let dir = temp_dir("volumes");
        std::fs::create_dir_all(dir.join("USB STICK")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("/", dir.join("Macintosh HD")).unwrap();
        let names: Vec<_> = drives(&dir).into_iter().map(|d| d.name).collect();
        assert_eq!(names, vec!["USB STICK"]);
    }

    #[test]
    fn a_partial_android_copy_is_kept_and_an_empty_one_fails() {
        let dir = temp_dir("pull");
        assert_eq!(android_pull_outcome(Ok(String::new()), &dir), Ok(None));
        assert!(android_pull_outcome(Err("adb: error: failed to stat".into()), &dir).is_err());
        assert!(android_pull_outcome(Err("Stopped.".into()), &dir).is_err());
        std::fs::create_dir_all(dir.join("sdcard/DCIM")).unwrap();
        let kept = android_pull_outcome(Err("adb: error: Permission denied".into()), &dir);
        assert!(kept.unwrap().unwrap().contains("could not be copied"));
        assert!(android_pull_outcome(Err("Stopped.".into()), &dir).is_err());
    }

    #[test]
    fn backup_folder_is_dated() {
        let t = crate::timeline::days_from_civil(2026, 11, 11) * 86_400 + 9 * 3600 + 5 * 60;
        assert_eq!(
            backup_folder(Path::new("/Users/me"), "Sam's iPhone", t),
            PathBuf::from("/Users/me/Banker Backups/Sam's iPhone 2026-11-11 09.05")
        );
    }
}
