//! What a path says about a file: whether it sits in a trash folder, and
//! which app it belongs to.

/// A file found in a trash or recovery folder.
#[derive(Debug, Clone, PartialEq)]
pub struct Trashed {
    pub reason: &'static str,
    /// The name without trash prefixes (Android's `.trashed-<time>-`).
    pub name: String,
    /// When it was deleted, if the path records it.
    pub deleted_at: Option<i64>,
}

/// Android keeps trashed media for 30 days and names the file after the
/// moment it will be purged.
const ANDROID_TRASH_DAYS: i64 = 30;

/// Housekeeping files that live in trash folders but are not user content.
fn housekeeping(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == ".ds_store"
        || lower == "desktop.ini"
        || lower == ".nomedia"
        || lower.starts_with("._")
        || lower.starts_with("$i") // Windows recycle-bin metadata; $R holds the content
}

/// Is the file at `path` (with `/` separators, relative to the device root)
/// in a place where deleted files are kept?
pub fn trashed(path: &str) -> Option<Trashed> {
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    let name = *parts.last()?;
    if housekeeping(name) {
        return None;
    }

    if let Some(rest) = name.strip_prefix(".trashed-") {
        // .trashed-1712345678-IMG_1234.jpg
        let (stamp, original) = rest.split_once('-')?;
        let expires: i64 = stamp.parse().ok()?;
        return Some(Trashed {
            reason: "In the phone's trash (deleted from Photos or Files)",
            name: original.to_string(),
            deleted_at: Some(expires - ANDROID_TRASH_DAYS * 86_400),
        });
    }

    let dirs = &parts[..parts.len() - 1];
    let in_dir = |pred: &dyn Fn(&str) -> bool| dirs.iter().any(|d| pred(d));
    let lower = |d: &str| d.to_ascii_lowercase();

    let reason = if in_dir(&|d| matches!(d, ".Trashes" | ".Trash")) {
        "In the Trash"
    } else if in_dir(&|d| matches!(lower(d).as_str(), "$recycle.bin" | "recycler")) {
        "In the Windows Recycle Bin"
    } else if in_dir(&|d| d.eq_ignore_ascii_case("LOST.DIR")) {
        "Recovered by Android after the card was removed unsafely"
    } else if in_dir(&|d| {
        let l = lower(d);
        l.len() == 9 && l.starts_with("found.") && l[6..].chars().all(|c| c.is_ascii_digit())
    }) {
        "Recovered by a disk check (pieces of damaged files)"
    } else if app_data_trash(dirs) {
        "In an app's own trash folder"
    } else {
        return None;
    };
    Some(Trashed {
        reason,
        name: name.to_string(),
        deleted_at: None,
    })
}

/// `Android/data/<package>/.../trash` style folders (Google Photos, Samsung
/// Gallery and file managers keep their own bins there).
fn app_data_trash(dirs: &[&str]) -> bool {
    let Some(i) = dirs.iter().position(|d| d.eq_ignore_ascii_case("Android")) else {
        return false;
    };
    dirs.len() > i + 3
        && dirs[i + 1].eq_ignore_ascii_case("data")
        && dirs[i + 3..].iter().any(|d| {
            let l = d.to_ascii_lowercase();
            matches!(
                l.as_str(),
                ".trash" | "trash" | ".recyclebin" | "recyclebin" | ".recycle" | "recently deleted"
            )
        })
}

/// Well-known top-level folders that belong to one app.
const FOLDER_APPS: &[(&str, &str)] = &[
    ("WhatsApp", "com.whatsapp"),
    ("Telegram", "org.telegram.messenger"),
    ("Snapchat", "com.snapchat.android"),
    ("Instagram", "com.instagram.android"),
    ("Messenger", "com.facebook.orca"),
    ("Viber", "com.viber.voip"),
    ("Signal", "org.thoughtcrime.securesms"),
    ("Screenshots", "Screenshots"),
    ("Camera", "Camera"),
    ("Recordings", "Recorder"),
];

/// Which app a file belongs to, judged from its path on the device.
pub fn app_for_path(path: &str) -> Option<String> {
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    let dirs = &parts[..parts.len().saturating_sub(1)];
    for (i, d) in dirs.iter().enumerate() {
        let next = dirs.get(i + 1).copied();
        let after = dirs.get(i + 2).copied();
        match (*d, next, after) {
            // Android: Android/data/<package>, Android/media/<package>
            // Samsung (Android 11+) keeps its bins in Android/.Trash/<package>.
            ("Android", Some("data" | "media" | "obb" | ".Trash"), Some(pkg))
                if pkg.contains('.') =>
            {
                return Some(pkg.to_string())
            }
            // macOS: ~/Library/Containers/<bundle id>
            ("Library", Some("Containers" | "Group Containers"), Some(id)) => {
                return Some(id.trim_start_matches("group.").to_string())
            }
            ("Library", Some("Application Support"), Some(name)) => return Some(name.to_string()),
            _ => {}
        }
        if let Some(app) = d.strip_suffix(".app") {
            return Some(app.to_string());
        }
        if d.ends_with(".photoslibrary") {
            return Some("Photos".to_string());
        }
    }
    for d in dirs {
        if let Some((_, app)) = FOLDER_APPS.iter().find(|(f, _)| d.eq_ignore_ascii_case(f)) {
            return Some(app.to_string());
        }
    }
    None
}

/// Which app a file in an iPhone backup belongs to, from its backup domain
/// and path. Apple's own apps live in HomeDomain and are told apart by path.
pub fn app_for_ios(domain: &str, relative_path: &str) -> Option<String> {
    for prefix in ["AppDomainGroup-group.", "AppDomainPlugin-", "AppDomain-"] {
        if let Some(id) = domain.strip_prefix(prefix) {
            return Some(match id {
                "com.apple.notes" => "Notes".to_string(),
                _ => id.to_string(),
            });
        }
    }
    let path = relative_path;
    let apple = [
        ("CameraRollDomain", "", "Photos"),
        ("MediaDomain", "Library/SMS/", "Messages"),
        ("HomeDomain", "Library/SMS/", "Messages"),
        ("HomeDomain", "Library/CallHistoryDB/", "Phone"),
        ("HomeDomain", "Library/AddressBook/", "Contacts"),
        ("HomeDomain", "Library/Calendar/", "Calendar"),
        ("HomeDomain", "Library/Notes/", "Notes"),
        ("HomeDomain", "Library/Safari/", "Safari"),
        ("HomeDomain", "Library/Voicemail/", "Voicemail"),
        ("HomeDomain", "Library/Recordings/", "Voice Memos"),
        ("HealthDomain", "", "Health"),
        ("KeychainDomain", "", "Keychain"),
    ];
    apple
        .iter()
        .find(|(d, p, _)| domain == *d && path.starts_with(p))
        .map(|(_, _, app)| app.to_string())
}

/// A readable name for an app identifier.
pub fn app_display_name(id: &str) -> String {
    const KNOWN: &[(&str, &str)] = &[
        ("com.whatsapp", "WhatsApp"),
        ("net.whatsapp.WhatsApp", "WhatsApp"),
        ("net.whatsapp.WhatsApp.shared", "WhatsApp"),
        ("org.telegram.messenger", "Telegram"),
        ("ph.telegra.Telegraph", "Telegram"),
        ("com.snapchat.android", "Snapchat"),
        ("com.toyopagroup.picaboo", "Snapchat"),
        ("com.instagram.android", "Instagram"),
        ("com.burbn.instagram", "Instagram"),
        ("com.facebook.orca", "Messenger"),
        ("com.facebook.Messenger", "Messenger"),
        ("com.facebook.katana", "Facebook"),
        ("com.facebook.Facebook", "Facebook"),
        ("com.viber.voip", "Viber"),
        ("org.thoughtcrime.securesms", "Signal"),
        ("org.whispersystems.signal", "Signal"),
        ("com.google.android.apps.photos", "Google Photos"),
        ("com.sec.android.gallery3d", "Samsung Gallery"),
        ("com.google.android.apps.messaging", "Google Messages"),
        ("com.zhiliaoapp.musically", "TikTok"),
        ("com.ss.iphone.ugc.Ame", "TikTok"),
        ("com.atebits.Tweetie2", "X"),
        ("com.twitter.android", "X"),
        ("com.discord", "Discord"),
        ("com.hammerandchisel.discord", "Discord"),
        ("com.tinyspeck.chatlyio", "Slack"),
        ("com.Slack", "Slack"),
    ];
    if let Some((_, name)) = KNOWN.iter().find(|(k, _)| *k == id) {
        return name.to_string();
    }
    if !id.contains('.') {
        return id.to_string();
    }
    // com.example.SomeApp -> SomeApp
    let last = id.rsplit('.').next().unwrap_or(id);
    let mut chars = last.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => id.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_trash_names_decode() {
        let t = trashed("DCIM/Camera/.trashed-1712592000-IMG_0042.jpg").unwrap();
        assert_eq!(t.name, "IMG_0042.jpg");
        assert_eq!(t.deleted_at, Some(1_712_592_000 - 30 * 86_400));
    }

    #[test]
    fn trash_folders() {
        assert!(trashed(".Trashes/501/photo.jpg").is_some());
        assert!(trashed("Users/me/.Trash/report.pdf").is_some());
        assert!(trashed("$RECYCLE.BIN/S-1-5/$RABC.docx").is_some());
        assert!(trashed("$RECYCLE.BIN/S-1-5/$IABC.docx").is_none());
        assert!(trashed("FOUND.000/FILE0001.CHK").is_some());
        assert!(trashed("LOST.DIR/1234").is_some());
        assert!(trashed("Android/data/com.google.android.apps.photos/files/trash/a.jpg").is_some());
        assert!(trashed(".Trashes/.DS_Store").is_none());
        assert!(trashed("Documents/Trash talk.txt").is_none());
        assert!(trashed("Pictures/holiday.jpg").is_none());
    }

    #[test]
    fn apps_from_paths() {
        assert_eq!(
            app_for_path("Android/media/com.whatsapp/WhatsApp/Media/x.jpg").as_deref(),
            Some("com.whatsapp")
        );
        assert_eq!(
            app_for_path("Users/me/Library/Containers/com.apple.Notes/Data/x").as_deref(),
            Some("com.apple.Notes")
        );
        assert_eq!(
            app_for_path("WhatsApp/Media/x.jpg").as_deref(),
            Some("com.whatsapp")
        );
        assert_eq!(app_for_path("random/file.txt"), None);
    }

    #[test]
    fn apps_from_ios_domains() {
        assert_eq!(
            app_for_ios("AppDomain-net.whatsapp.WhatsApp", "Documents/x").as_deref(),
            Some("net.whatsapp.WhatsApp")
        );
        assert_eq!(
            app_for_ios("HomeDomain", "Library/SMS/sms.db").as_deref(),
            Some("Messages")
        );
        assert_eq!(
            app_for_ios("CameraRollDomain", "Media/DCIM/100APPLE/IMG_1.HEIC").as_deref(),
            Some("Photos")
        );
        assert_eq!(app_for_ios("HomeDomain", "Library/Preferences/x"), None);
    }

    #[test]
    fn display_names() {
        assert_eq!(app_display_name("net.whatsapp.WhatsApp"), "WhatsApp");
        assert_eq!(app_display_name("com.example.coolApp"), "CoolApp");
        assert_eq!(app_display_name("Photos"), "Photos");
    }
}
