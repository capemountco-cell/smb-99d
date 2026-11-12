//! Finding connected devices and copying them to the Mac.
//!
//! * Drives (USB disks, SD cards, cameras) are mounted under /Volumes and
//!   are scanned in place, read-only.
//! * iPhones are backed up with `idevicebackup2` (libimobiledevice), then the
//!   backup is scanned.
//! * Android phones are copied with `adb pull`, then the copy is scanned.
//! * Existing Finder backups of iPhones can be scanned directly.

use std::collections::HashSet;
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

/// Mount points of disk images (`.dmg` installers and the like) from
/// `hdiutil info`. These sit in /Volumes beside real drives but are not
/// devices anyone wants to recover from.
pub fn parse_disk_image_mounts(hdiutil_info: &str) -> HashSet<PathBuf> {
    hdiutil_info
        .lines()
        .filter_map(|line| {
            line.split('\t')
                .map(str::trim)
                .find(|t| t.starts_with("/Volumes/"))
        })
        .map(PathBuf::from)
        .collect()
}

/// iPhones and iPads plugged in by USB, from `ioreg -p IOUSB -l -w 0`, as
/// (product, UDID). macOS sees them even when libimobiledevice is missing,
/// so Banker can say what is needed instead of not listing the phone.
pub fn parse_usb_apple_devices(ioreg: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut product: Option<String> = None;
    let mut serial: Option<String> = None;
    let value = |line: &str| {
        line.split_once('=')
            .map(|(_, v)| v.trim().trim_matches('"').to_string())
    };
    let mut flush = |product: &mut Option<String>, serial: &mut Option<String>| {
        if let (Some(p), Some(s)) = (product.take(), serial.take()) {
            if matches!(p.as_str(), "iPhone" | "iPad" | "iPod") {
                // Newer devices report a 24-digit serial; their UDID has a
                // dash after the first eight.
                let udid = if s.len() == 24 && !s.contains('-') {
                    format!("{}-{}", &s[..8], &s[8..])
                } else {
                    s
                };
                found.push((p, udid));
            }
        }
    };
    for line in ioreg.lines() {
        if line.contains("+-o ") {
            flush(&mut product, &mut serial);
        } else if line.contains("\"USB Product Name\"") {
            product = value(line);
        } else if line.contains("\"USB Serial Number\"") {
            serial = value(line);
        }
    }
    flush(&mut product, &mut serial);
    found
}

/// Mounted drives under `volumes`, leaving out the Mac's own disk and the
/// disk images in `images`.
pub fn drives(volumes: &Path, images: &HashSet<PathBuf>) -> Vec<Device> {
    let Ok(entries) = std::fs::read_dir(volumes) else {
        return Vec::new();
    };
    let mut out: Vec<Device> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let path = e.path();
            let real = std::fs::canonicalize(&path).ok()?;
            if real == Path::new("/")
                || !real.is_dir()
                || images.contains(&path)
                || images.contains(&real)
            {
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

pub(crate) fn output(tool: &Path, args: &[&str]) -> Option<String> {
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
    let images = Path::new("/usr/bin/hdiutil")
        .is_file()
        .then(|| output(Path::new("/usr/bin/hdiutil"), &["info"]))
        .flatten()
        .map(|o| parse_disk_image_mounts(&o))
        .unwrap_or_default();
    d.devices.extend(drives(Path::new("/Volumes"), &images));
    let usb_apple = Path::new("/usr/sbin/ioreg")
        .is_file()
        .then(|| {
            output(
                Path::new("/usr/sbin/ioreg"),
                &["-p", "IOUSB", "-l", "-w", "0"],
            )
        })
        .flatten()
        .map(|o| parse_usb_apple_devices(&o))
        .unwrap_or_default();
    let has_brew = find_tool("brew").is_some();

    let idevice = find_tool("idevice_id");
    match idevice {
        Some(ref tool) => {
            let info = find_tool("ideviceinfo");
            for udid in parse_idevice_ids(&output(tool, &["-l"]).unwrap_or_default()) {
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
            text: if has_brew {
                "To read iPhones and iPads, install libimobiledevice. In Terminal:".into()
            } else {
                "To read iPhones and iPads, install Homebrew from brew.sh, then in Terminal:".into()
            },
            command: Some("brew install libimobiledevice".into()),
        }),
    }
    // A phone macOS sees on USB but Banker cannot read yet: list it, with
    // what to do, rather than leaving it out.
    for (product, udid) in usb_apple {
        if d.devices.iter().any(|x| x.id == udid) {
            continue;
        }
        d.devices.push(Device {
            id: udid,
            kind: DeviceKind::Iphone,
            name: product.clone(),
            detail: format!("{product} · connected by USB"),
            path: None,
            problem: Some(if idevice.is_none() {
                format!("Banker needs libimobiledevice to read this {product}. See \"Not seeing a device?\".")
            } else {
                format!("Unlock the {product} and tap Trust, then wait a few seconds.")
            }),
        });
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
    run_with(tool, args, &[], dest, cancel, &mut |_| {})
}

/// `run`, plus environment variables (for secrets, which must not appear in
/// the process list) and a callback with each new piece of output, for
/// tools that print progress.
pub(crate) fn run_with(
    tool: &Path,
    args: &[&str],
    env: &[(&str, &str)],
    log_dir: &Path,
    cancel: &AtomicBool,
    on_output: &mut dyn FnMut(&str),
) -> Result<String, String> {
    std::fs::create_dir_all(log_dir)
        .map_err(|e| format!("Could not create {}: {e}", log_dir.display()))?;
    let log_path = log_dir.join("banker-copy-log.txt");
    let log = std::fs::File::create(&log_path).map_err(|e| e.to_string())?;
    let err = log.try_clone().map_err(|e| e.to_string())?;
    let mut child = Command::new(tool)
        .args(args)
        .envs(env.iter().copied())
        .stdin(Stdio::null())
        .stdout(log)
        .stderr(err)
        .spawn()
        .map_err(|e| format!("Could not start {}: {e}", tool.display()))?;
    let mut seen = 0usize;
    let mut read_new = |on_output: &mut dyn FnMut(&str)| {
        if let Ok(bytes) = std::fs::read(&log_path) {
            if bytes.len() > seen {
                on_output(&String::from_utf8_lossy(&bytes[seen..]));
                seen = bytes.len();
            }
        }
    };
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Stopped.".into());
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                read_new(on_output);
                std::thread::sleep(Duration::from_millis(400));
            }
            Err(e) => return Err(e.to_string()),
        }
    };
    read_new(on_output);
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

/// The last "NN%" a progress-printing tool wrote.
pub fn last_percent(output: &str) -> Option<u8> {
    let bytes = output.as_bytes();
    let mut i = bytes.len();
    while i > 0 {
        i -= 1;
        if bytes[i] != b'%' {
            continue;
        }
        let digits_start = bytes[..i]
            .iter()
            .rposition(|b| !b.is_ascii_digit())
            .map_or(0, |p| p + 1);
        if digits_start < i {
            if let Ok(n) = output[digits_start..i].parse::<u16>() {
                if n <= 100 {
                    return Some(n as u8);
                }
            }
        }
    }
    None
}

/// Make sure the iPhone trusts this Mac, asking it to if not.
pub(crate) fn pair_iphone(udid: &str, log_dir: &Path, cancel: &AtomicBool) -> Result<(), String> {
    let pair = find_tool("idevicepair").ok_or("Install libimobiledevice first.")?;
    if run(&pair, &["-u", udid, "validate"], log_dir, cancel).is_ok() {
        return Ok(());
    }
    run(&pair, &["-u", udid, "pair"], log_dir, cancel)
        .map(|_| ())
        .map_err(|e| {
            if e.contains("trust") || e.contains("passcode") || e.contains("Trust") {
                "On the iPhone, unlock it, tap Trust and enter the passcode, then try again.".into()
            } else {
                format!("The iPhone did not pair with this Mac: {e}")
            }
        })
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
            let backup = find_tool("idevicebackup2").ok_or("Install libimobiledevice first.")?;
            pair_iphone(&device.id, dest, cancel)?;
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
        let names: Vec<_> = drives(&dir, &HashSet::new())
            .into_iter()
            .map(|d| d.name)
            .collect();
        assert_eq!(names, vec!["USB STICK"]);

        std::fs::create_dir_all(dir.join("Some Installer")).unwrap();
        let images: HashSet<PathBuf> = [dir.join("Some Installer")].into_iter().collect();
        let names: Vec<_> = drives(&dir, &images).into_iter().map(|d| d.name).collect();
        assert_eq!(names, vec!["USB STICK"], "disk images are not drives");
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
    fn finds_disk_image_mounts() {
        let info = "framework       : 666\n================================================\n\
                    image-path      : /Users/me/Downloads/Banker.dmg\n\
                    /dev/disk4\tGUID_partition_scheme\t\n\
                    /dev/disk4s1\tApple_HFS\t/Volumes/Banker\n\
                    /dev/disk5s1\tApple_HFS\t/Volumes/Adobe Illustrator 2020 v24.0.1\n";
        let mounts = parse_disk_image_mounts(info);
        assert!(mounts.contains(Path::new("/Volumes/Banker")));
        assert!(mounts.contains(Path::new("/Volumes/Adobe Illustrator 2020 v24.0.1")));
        assert_eq!(mounts.len(), 2);
    }

    #[test]
    fn finds_iphones_on_usb() {
        let ioreg = "+-o Root  <class IORegistryEntry>\n\
                       +-o iPhone@01100000  <class IOUSBHostDevice>\n\
                         |   \"USB Product Name\" = \"iPhone\"\n\
                         |   \"USB Serial Number\" = \"00008030001A2B3C4D5E802E\"\n\
                       +-o USB Keyboard@02100000  <class IOUSBHostDevice>\n\
                         |   \"USB Product Name\" = \"USB Keyboard\"\n\
                         |   \"USB Serial Number\" = \"123\"\n";
        assert_eq!(
            parse_usb_apple_devices(ioreg),
            vec![(
                "iPhone".to_string(),
                "00008030-001A2B3C4D5E802E".to_string()
            )]
        );
    }

    #[test]
    fn reads_progress_percentages() {
        assert_eq!(
            last_percent("[==   ] 12% Finished\n[=====] 57% Fin"),
            Some(57)
        );
        assert_eq!(last_percent("no numbers"), None);
        assert_eq!(last_percent("250% nonsense 9%"), Some(9));
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
