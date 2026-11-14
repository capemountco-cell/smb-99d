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
    /// The Mac Reco runs on, read where it is.
    Mac,
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

/// A phone macOS sees on USB, whether or not Banker's tools can talk to it.
#[derive(Debug, Clone, PartialEq)]
pub struct UsbPhone {
    pub kind: DeviceKind,
    pub product: String,
    /// The iPhone's UDID, or the Android phone's serial (which adb uses too).
    pub id: String,
}

/// USB vendor ids of Android phone makers: Google, Samsung, Motorola, LG,
/// HTC, Sony, OnePlus, Xiaomi, Huawei, OPPO/Realme, Vivo, Nothing, ZTE,
/// Lenovo, Asus, Nokia (HMD), Essential, Fairphone.
const ANDROID_VENDORS: &[u32] = &[
    0x18d1, 0x04e8, 0x22b8, 0x1004, 0x0bb4, 0x0fce, 0x2a70, 0x2717, 0x12d1, 0x22d9, 0x2d95, 0x2b4c,
    0x19d2, 0x17ef, 0x0b05, 0x2e04, 0x2e17, 0x2ae5,
];
const APPLE_VENDOR: u32 = 0x05ac;

/// Phones plugged in by USB, from `ioreg -p IOUSB -l -w 0`. macOS lists them
/// even when no phone tool works, so Banker can always show the phone and
/// say what it needs, instead of leaving it out. Reads both the older and
/// newer names macOS uses for the product, serial and vendor.
pub fn parse_usb_phones(ioreg: &str) -> Vec<UsbPhone> {
    let mut found = Vec::new();
    let mut product: Option<String> = None;
    let mut serial: Option<String> = None;
    let mut vendor: Option<u32> = None;
    let value = |line: &str| {
        line.split_once('=')
            .map(|(_, v)| v.trim().trim_matches('"').to_string())
    };
    let mut flush =
        |product: &mut Option<String>, serial: &mut Option<String>, vendor: &mut Option<u32>| {
            let (p, s, v) = (product.take(), serial.take(), vendor.take());
            let (Some(p), Some(s)) = (p, s) else { return };
            let apple_name = ["iPhone", "iPad", "iPod"].iter().any(|n| p.starts_with(n));
            if apple_name && v.is_none_or(|v| v == APPLE_VENDOR) {
                // Newer devices report a 24-digit serial; their UDID has a dash
                // after the first eight.
                let id = if s.len() == 24 && !s.contains('-') {
                    format!("{}-{}", &s[..8], &s[8..])
                } else {
                    s
                };
                found.push(UsbPhone {
                    kind: DeviceKind::Iphone,
                    product: p,
                    id,
                });
            } else if v.is_some_and(|v| ANDROID_VENDORS.contains(&v)) {
                found.push(UsbPhone {
                    kind: DeviceKind::Android,
                    product: p.replace('_', " "),
                    id: s,
                });
            }
        };
    for line in ioreg.lines() {
        if line.contains("+-o ") {
            flush(&mut product, &mut serial, &mut vendor);
        } else if line.contains("\"USB Product Name\"") || line.contains("\"kUSBProductString\"") {
            product = product.or(value(line));
        } else if line.contains("\"USB Serial Number\"")
            || line.contains("\"kUSBSerialNumberString\"")
        {
            serial = serial.or(value(line));
        } else if line.contains("\"idVendor\"") {
            vendor = value(line).and_then(|v| v.parse().ok());
        }
    }
    flush(&mut product, &mut serial, &mut vendor);
    found
}

/// What `diskutil info <volume>` says decides it: a disk image, however it
/// was mounted.
pub fn diskutil_says_disk_image(info: &str) -> bool {
    info.lines().any(|l| {
        let l = l.trim();
        l.starts_with("Protocol:") && l.contains("Disk Image")
    })
}

/// An installer laid out the usual way: an app (or a .pkg), maybe an
/// Applications shortcut and a few documents, and nothing else.
pub fn looks_like_installer(volume: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(volume) else {
        return false;
    };
    let mut apps = 0;
    for e in entries.filter_map(Result::ok) {
        let name = e.file_name().to_string_lossy().to_ascii_lowercase();
        if name.starts_with('.') {
            continue;
        }
        let ext = name.rsplit_once('.').map(|(_, x)| x).unwrap_or("");
        match ext {
            "app" | "pkg" | "mpkg" => apps += 1,
            "pdf" | "rtf" | "rtfd" | "txt" | "html" | "png" | "webloc" | "command" => {}
            _ if name == "applications" => {}
            _ => return false,
        }
    }
    apps > 0
}

/// Mounted drives under `volumes`, leaving out the Mac's own disk and every
/// disk image: those `hdiutil` lists in `images`, those `is_image` (Disk
/// Utility) reports, and anything laid out like an app installer. An app is
/// never offered as a device.
pub fn drives(
    volumes: &Path,
    images: &HashSet<PathBuf>,
    is_image: &dyn Fn(&Path) -> bool,
) -> Vec<Device> {
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
                || looks_like_installer(&path)
                || is_image(&path)
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
                detail: "Drive · read in place".into(),
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
    // The copies bundled inside Banker.app come first, so phones work on a
    // Mac with nothing installed. The app sets this at startup.
    let bundled = std::env::var_os("BANKER_TOOLS_DIR").map(PathBuf::from);
    bundled
        .into_iter()
        .chain(std::env::split_paths(&path))
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

/// The startup disk's name in `diskutil info /` output.
pub fn parse_volume_name(info: &str) -> Option<String> {
    info.lines()
        .find_map(|l| l.trim().strip_prefix("Volume Name:"))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// The Mac itself. It is always listed first and read where it is, so a
/// scan of it never needs space.
pub fn this_mac(home: &Path) -> Device {
    let diskutil = Path::new("/usr/sbin/diskutil");
    let volume = diskutil
        .is_file()
        .then(|| output(diskutil, &["info", "/"]))
        .flatten()
        .and_then(|i| parse_volume_name(&i))
        .unwrap_or_else(|| "Macintosh HD".into());
    Device {
        id: "mac".into(),
        kind: DeviceKind::Mac,
        name: "This Mac".into(),
        detail: format!("{volume} · read in place"),
        path: Some(home.display().to_string()),
        problem: None,
    }
}

/// Everything connected or available right now.
pub fn discover(home: &Path) -> Discovery {
    let mut d = Discovery::default();
    d.devices.push(this_mac(home));
    let images = Path::new("/usr/bin/hdiutil")
        .is_file()
        .then(|| output(Path::new("/usr/bin/hdiutil"), &["info"]))
        .flatten()
        .map(|o| parse_disk_image_mounts(&o))
        .unwrap_or_default();
    let diskutil = Path::new("/usr/sbin/diskutil");
    let is_image = |volume: &Path| {
        diskutil.is_file()
            && output(diskutil, &["info", &volume.display().to_string()])
                .is_some_and(|info| diskutil_says_disk_image(&info))
    };
    d.devices
        .extend(drives(Path::new("/Volumes"), &images, &is_image));
    let usb_apple = Path::new("/usr/sbin/ioreg")
        .is_file()
        .then(|| {
            output(
                Path::new("/usr/sbin/ioreg"),
                &["-p", "IOUSB", "-l", "-w", "0"],
            )
        })
        .flatten()
        .map(|o| parse_usb_phones(&o))
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
                    detail: "iPhone · USB".into(),
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
                    detail: "Android · USB".into(),
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

    // A phone macOS sees on USB that Banker's tools did not list: show it
    // anyway, with what to do, rather than leaving it out.
    let adb_found = find_tool("adb").is_some();
    for phone in usb_apple {
        if d.devices.iter().any(|x| x.id == phone.id) {
            continue;
        }
        let product = phone.product.clone();
        let problem = match phone.kind {
            DeviceKind::Iphone if idevice.is_none() => {
                "Reco cannot find its iPhone tools. Reinstall Reco, or see \"Not seeing a device?\".".to_string()
            }
            DeviceKind::Iphone => format!("Unlock the {product} and tap Trust, then wait a few seconds."),
            _ if !adb_found => "Reco cannot find its Android tools. Reinstall Reco, or see \"Not seeing a device?\".".to_string(),
            _ => "Turn on USB debugging: Settings › About phone, tap Build number 7 times, then Developer options › USB debugging. Then tap Allow on the phone.".to_string(),
        };
        d.devices.push(Device {
            id: phone.id,
            kind: phone.kind,
            name: product.clone(),
            detail: format!("{product} · USB"),
            path: None,
            problem: Some(problem),
        });
    }

    d.devices.extend(finder_backups(
        &home.join("Library/Application Support/MobileSync/Backup"),
    ));
    d
}

/// `<save root>/<name> <YYYY-MM-DD HH.MM>`, the save root being
/// `~/Banker Backups` or the folder chosen in settings.
pub fn backup_folder(root: &Path, device_name: &str, now_local: i64) -> PathBuf {
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
    root.join(format!(
        "{safe} {y}-{m:02}-{d:02} {:02}.{:02}",
        secs / 3600,
        secs % 3600 / 60
    ))
}

/// Run a tool until it finishes or the user cancels. Its output goes to a
/// log file in `dest`, so a chatty tool can never fill a pipe and stall.
pub(crate) fn run(
    tool: &Path,
    args: &[&str],
    dest: &Path,
    cancel: &AtomicBool,
) -> Result<String, String> {
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

/// idevicebackup2's failure, in words the person can act on.
pub fn backup_error(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    if lower.contains("105") || lower.contains("insufficient free disk space") {
        "Not enough space to copy the iPhone. Choose another Save to location (an external drive works) under Backups, or free up space, then try again.".into()
    } else if lower.contains("passcode") || lower.contains("locked") {
        "The iPhone is locked. Unlock it, keep it unlocked, and try again.".into()
    } else {
        format!("Backing up the iPhone failed: {raw}")
    }
}

/// How much is stored on a phone, to check a copy will fit before starting.
pub fn phone_used_bytes(device: &Device) -> Option<u64> {
    match device.kind {
        DeviceKind::Iphone => {
            let info = find_tool("ideviceinfo")?;
            let read = |key: &str| -> Option<u64> {
                output(
                    &info,
                    &["-u", &device.id, "-q", "com.apple.disk_usage", "-k", key],
                )?
                .trim()
                .parse()
                .ok()
            };
            Some(read("TotalDataCapacity")?.saturating_sub(read("TotalDataAvailable")?))
        }
        DeviceKind::Android => {
            let adb = find_tool("adb")?;
            let df = output(&adb, &["-s", &device.id, "shell", "df", "-k", "/sdcard"])?;
            let line = df.lines().rev().find(|l| !l.trim().is_empty())?;
            let used_kb: u64 = line.split_whitespace().nth(2)?.parse().ok()?;
            Some(used_kb * 1024)
        }
        _ => None,
    }
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
        DeviceKind::Mac | DeviceKind::Drive | DeviceKind::Backup | DeviceKind::Folder => device
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
            // A backup already there is brought up to date: only what changed
            // on the phone is copied, which is faster and needs no new space.
            let fresh = !dest.join(&device.id).join("Manifest.db").is_file();
            let mut args = vec!["-u", device.id.as_str(), "backup"];
            if fresh {
                args.push("--full");
            }
            args.push(&target);
            run(&backup, &args, dest, cancel).map_err(|e| backup_error(&e))?;
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
    fn this_mac_comes_first_and_reads_in_place() {
        let home = temp_dir("mac");
        let d = discover(&home);
        assert_eq!(d.devices[0].kind, DeviceKind::Mac);
        assert_eq!(d.devices[0].path.as_deref(), Some(home.to_str().unwrap()));
        assert_eq!(
            parse_volume_name(
                "   Device Node: /dev/disk3s1\n   Volume Name:              Macintosh HD\n"
            ),
            Some("Macintosh HD".into())
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
        let names: Vec<_> = drives(&dir, &HashSet::new(), &|_| false)
            .into_iter()
            .map(|d| d.name)
            .collect();
        assert_eq!(names, vec!["USB STICK"]);

        std::fs::create_dir_all(dir.join("Some Installer")).unwrap();
        let images: HashSet<PathBuf> = [dir.join("Some Installer")].into_iter().collect();
        let names: Vec<_> = drives(&dir, &images, &|_| false)
            .into_iter()
            .map(|d| d.name)
            .collect();
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
    fn finds_phones_on_usb() {
        let ioreg = "+-o Root  <class IORegistryEntry>\n\
                       +-o iPhone@01100000  <class IOUSBHostDevice>\n\
                         |   \"USB Product Name\" = \"iPhone\"\n\
                         |   \"USB Serial Number\" = \"00008030001A2B3C4D5E802E\"\n\
                       +-o USB Keyboard@02100000  <class IOUSBHostDevice>\n\
                         |   \"USB Product Name\" = \"USB Keyboard\"\n\
                         |   \"USB Serial Number\" = \"123\"\n";
        assert_eq!(
            parse_usb_phones(ioreg),
            vec![UsbPhone {
                kind: DeviceKind::Iphone,
                product: "iPhone".into(),
                id: "00008030-001A2B3C4D5E802E".into()
            }]
        );

        // Newer macOS names, and an Android phone (Google's vendor id).
        let newer = "+-o Pixel 8@00100000  <class IOUSBHostDevice>\n\
                       |   \"idVendor\" = 6353\n\
                       |   \"kUSBProductString\" = \"Pixel_8\"\n\
                       |   \"kUSBSerialNumberString\" = \"38051FDJH000AB\"\n\
                     +-o iPad@00200000  <class IOUSBHostDevice>\n\
                       |   \"idVendor\" = 1452\n\
                       |   \"kUSBProductString\" = \"iPad\"\n\
                       |   \"kUSBSerialNumberString\" = \"0000811000AA22BB33CC44DD\"\n\
                     +-o Mouse@00300000  <class IOUSBHostDevice>\n\
                       |   \"idVendor\" = 1133\n\
                       |   \"kUSBProductString\" = \"Mouse\"\n\
                       |   \"kUSBSerialNumberString\" = \"9\"\n";
        let phones = parse_usb_phones(newer);
        assert_eq!(phones.len(), 2);
        assert_eq!(phones[0].kind, DeviceKind::Android);
        assert_eq!(phones[0].product, "Pixel 8");
        assert_eq!(phones[0].id, "38051FDJH000AB");
        assert_eq!(phones[1].kind, DeviceKind::Iphone);
        assert_eq!(phones[1].id, "00008110-00AA22BB33CC44DD");
    }

    #[test]
    fn an_app_installer_is_never_a_drive() {
        let dir = temp_dir("installers");
        std::fs::create_dir_all(dir.join("Banker/Banker.app/Contents")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("/Applications", dir.join("Banker/Applications")).unwrap();
        std::fs::create_dir_all(dir.join("Tool Installer/Install Tool.pkg")).unwrap();
        std::fs::write(dir.join("Tool Installer/Read Me.pdf"), b"%PDF").unwrap();
        std::fs::create_dir_all(dir.join("USB STICK/DCIM")).unwrap();
        std::fs::create_dir_all(dir.join("Odd Image/Stuff")).unwrap();
        let reported = dir.join("Odd Image");
        let is_image = move |p: &Path| p == reported;
        let names: Vec<_> = drives(&dir, &HashSet::new(), &is_image)
            .into_iter()
            .map(|d| d.name)
            .collect();
        assert_eq!(names, vec!["USB STICK"]);

        assert!(diskutil_says_disk_image(
            "   Protocol:                  Disk Image\n"
        ));
        assert!(!diskutil_says_disk_image(
            "   Protocol:                  USB\n"
        ));
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
    fn explains_backup_failures() {
        let e = backup_error(
            "ErrorCode 105: Insufficient free disk space on drive to back up (MBErrorDomain/105)",
        );
        assert!(e.contains("Not enough space"));
        assert!(e.contains("Save to"));
        assert!(backup_error("Device is locked with a passcode").contains("Unlock"));
        assert!(backup_error("odd").contains("odd"));
    }

    #[test]
    fn backup_folder_is_dated() {
        let t = crate::timeline::days_from_civil(2026, 11, 11) * 86_400 + 9 * 3600 + 5 * 60;
        assert_eq!(
            backup_folder(Path::new("/Users/me/Banker Backups"), "Sam's iPhone", t),
            PathBuf::from("/Users/me/Banker Backups/Sam's iPhone 2026-11-11 09.05")
        );
    }
}
