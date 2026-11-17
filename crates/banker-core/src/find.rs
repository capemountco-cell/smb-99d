//! Find: any file on this Mac by what the person remembers, in plain words
//! ("pdfs from gmail last week", "videos over 1 GB", "photo of a receipt").
//!
//! The words are turned into an `Understood`: kinds, where a download came
//! from, a date range, sizes, a place, words to look for. The window shows
//! each piece as a chip the person can drop. Live files come from Spotlight
//! (`mdfind`), asked fresh each time so there is no index of Reco's own to
//! keep in sync; Spotlight also reads text inside documents, PDFs and, on
//! recent macOS, pictures. Deleted files come from the Trash and from the
//! last Reco scan, and are marked so the window can offer Recover.

use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

use crate::model::{media_for_name, Media, Report};
use crate::paths::trashed;
use crate::timeline::{civil_from_days, days_from_civil};

/// The default shortcut. ⌘Space is Spotlight's, so never that.
pub const DEFAULT_SHORTCUT: &str = "Alt+Space";
const MAX_RESULTS: usize = 120;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Pdf,
    Photo,
    Screenshot,
    Video,
    Audio,
    Document,
    Spreadsheet,
    Presentation,
    Archive,
    Text,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Pdf => "PDF",
            Kind::Photo => "Photo",
            Kind::Screenshot => "Screenshot",
            Kind::Video => "Video",
            Kind::Audio => "Audio",
            Kind::Document => "Document",
            Kind::Spreadsheet => "Spreadsheet",
            Kind::Presentation => "Presentation",
            Kind::Archive => "Archive",
            Kind::Text => "Text",
        }
    }

    /// Spotlight's content type for it.
    fn uti(self) -> &'static str {
        match self {
            Kind::Pdf => "com.adobe.pdf",
            Kind::Photo | Kind::Screenshot => "public.image",
            Kind::Video => "public.movie",
            Kind::Audio => "public.audio",
            Kind::Document => "public.composite-content",
            Kind::Spreadsheet => "public.spreadsheet",
            Kind::Presentation => "public.presentation",
            Kind::Archive => "public.archive",
            Kind::Text => "public.plain-text",
        }
    }

    /// Does a file name fit this kind? Used for deleted files, which
    /// Spotlight does not know about.
    fn fits(self, name: &str) -> bool {
        let ext = name
            .rsplit_once('.')
            .map(|(_, e)| e.to_ascii_lowercase())
            .unwrap_or_default();
        match self {
            Kind::Pdf => ext == "pdf",
            Kind::Photo | Kind::Screenshot => media_for_name(name) == Media::Image,
            Kind::Video => media_for_name(name) == Media::Video,
            Kind::Audio => media_for_name(name) == Media::Audio,
            Kind::Document => matches!(
                ext.as_str(),
                "pdf" | "doc" | "docx" | "pages" | "rtf" | "odt" | "txt" | "md"
            ),
            Kind::Spreadsheet => matches!(ext.as_str(), "xls" | "xlsx" | "numbers" | "csv"),
            Kind::Presentation => matches!(ext.as_str(), "ppt" | "pptx" | "key"),
            Kind::Archive => media_for_name(name) == Media::Archive,
            Kind::Text => matches!(ext.as_str(), "txt" | "md" | "rtf"),
        }
    }
}

/// One piece of what was understood, shown as a chip. `key` is what the
/// window sends back to drop it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    pub key: String,
    pub label: String,
    pub value: String,
}

/// A rough box around a country, by latitude and longitude. Border areas
/// can fall in two boxes, so a place match is called approximate.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    pub name: String,
    pub lat: (f64, f64),
    pub lon: (f64, f64),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Understood {
    pub kinds: Vec<Kind>,
    /// Where a download came from: patterns for its web address.
    pub from: Option<(String, Vec<String>)>,
    /// Unix seconds, from inclusive, to exclusive.
    pub after: Option<i64>,
    pub before: Option<i64>,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    pub place: Option<Place>,
    /// Each group is alternatives; every group must match the name or text.
    pub words: Vec<Vec<String>>,
    pub only_deleted: bool,
    pub parts: Vec<Part>,
}

impl Understood {
    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
            && self.from.is_none()
            && self.after.is_none()
            && self.before.is_none()
            && self.min_size.is_none()
            && self.max_size.is_none()
            && self.place.is_none()
            && self.words.is_empty()
            && !self.only_deleted
    }
}

const STOP: &[&str] = &[
    "a",
    "an",
    "the",
    "of",
    "from",
    "in",
    "on",
    "at",
    "with",
    "my",
    "me",
    "file",
    "files",
    "that",
    "which",
    "show",
    "find",
    "all",
    "some",
    "for",
    "about",
    "to",
    "and",
    "i",
    "was",
    "were",
    "is",
    "it",
    "any",
    "by",
    "taken",
    "made",
    "sent",
    "got",
    "downloaded",
    "saved",
    "where",
    "what",
    "when",
    "had",
    "have",
    "there",
    "this",
    "those",
    "these",
    "one",
    "ones",
    "called",
    "named",
];

/// Words that name a kind of file.
fn kind_word(w: &str) -> Option<Kind> {
    Some(match w {
        "pdf" | "pdfs" => Kind::Pdf,
        "photo" | "photos" | "picture" | "pictures" | "pic" | "pics" | "image" | "images"
        | "selfie" | "selfies" => Kind::Photo,
        "screenshot" | "screenshots" => Kind::Screenshot,
        "video" | "videos" | "movie" | "movies" | "clip" | "clips" | "recording" | "recordings" => {
            Kind::Video
        }
        "audio" | "song" | "songs" | "music" | "mp3" | "mp3s" | "podcast" | "voice" => Kind::Audio,
        "document" | "documents" | "doc" | "docs" | "word" => Kind::Document,
        "spreadsheet" | "spreadsheets" | "excel" | "sheet" | "sheets" | "xls" | "xlsx" | "csv" => {
            Kind::Spreadsheet
        }
        "presentation" | "presentations" | "slides" | "deck" | "decks" | "keynote"
        | "powerpoint" => Kind::Presentation,
        "zip" | "zips" | "archive" | "archives" => Kind::Archive,
        "note" | "notes" | "txt" => Kind::Text,
        _ => return None,
    })
}

/// Where downloads come from, by the name people use.
fn source(w: &str) -> Option<(&'static str, &'static [&'static str])> {
    Some(match w {
        "gmail" | "google mail" => (
            "Gmail",
            &["mail.google.com", "mail-attachment.googleusercontent.com"],
        ),
        "outlook" | "hotmail" => (
            "Outlook",
            &[
                "outlook.live.com",
                "outlook.office",
                "attachments.office.net",
            ],
        ),
        "drive" | "google drive" => (
            "Google Drive",
            &[
                "drive.google.com",
                "docs.google.com",
                "googleusercontent.com",
            ],
        ),
        "dropbox" => ("Dropbox", &["dropbox.com", "dropboxusercontent.com"]),
        "slack" => ("Slack", &["slack.com", "slack-edge.com", "slack-files.com"]),
        "whatsapp" => ("WhatsApp", &["whatsapp.net", "whatsapp.com"]),
        "telegram" => ("Telegram", &["telegram.org", "t.me"]),
        "icloud" => ("iCloud", &["icloud.com"]),
        "wetransfer" => ("WeTransfer", &["wetransfer.com"]),
        "amazon" => ("Amazon", &["amazon."]),
        "github" => ("GitHub", &["github.com", "githubusercontent.com"]),
        _ => return None,
    })
}

/// Countries, roughly. Enough for "photos from Greece".
/// (name, latitude range, longitude range).
type Country = (&'static str, (f64, f64), (f64, f64));

const COUNTRIES: &[Country] = &[
    ("Greece", (34.8, 41.8), (19.3, 29.7)),
    ("Italy", (36.6, 47.1), (6.6, 18.6)),
    ("France", (41.3, 51.1), (-5.2, 9.6)),
    ("Spain", (35.9, 43.8), (-9.4, 3.4)),
    ("Portugal", (36.9, 42.2), (-9.6, -6.2)),
    ("England", (49.9, 55.8), (-6.4, 1.8)),
    ("Scotland", (54.6, 60.9), (-7.7, -0.7)),
    ("Ireland", (51.4, 55.4), (-10.5, -6.0)),
    ("Germany", (47.3, 55.1), (5.9, 15.0)),
    ("Netherlands", (50.8, 53.6), (3.3, 7.2)),
    ("Croatia", (42.4, 46.6), (13.5, 19.5)),
    ("Turkey", (36.0, 42.1), (26.0, 44.8)),
    ("Iceland", (63.3, 66.6), (-24.5, -13.5)),
    ("Morocco", (27.7, 35.9), (-13.2, -1.0)),
    ("Egypt", (22.0, 31.7), (24.7, 36.9)),
    ("South Africa", (-34.8, -22.1), (16.5, 32.9)),
    ("Kenya", (-4.7, 5.0), (33.9, 41.9)),
    ("India", (6.7, 35.5), (68.1, 97.4)),
    ("Thailand", (5.6, 20.5), (97.3, 105.6)),
    ("Japan", (24.0, 45.6), (122.9, 145.8)),
    ("Australia", (-43.7, -10.6), (113.3, 153.6)),
    ("New Zealand", (-47.3, -34.4), (166.4, 178.6)),
    ("Canada", (41.7, 83.1), (-141.0, -52.6)),
    ("USA", (24.5, 49.4), (-124.8, -66.9)),
    ("Mexico", (14.5, 32.7), (-118.4, -86.7)),
    ("Brazil", (-33.8, 5.3), (-73.99, -34.8)),
];

fn country(words: &[&str], i: usize) -> Option<(Place, usize)> {
    for (name, lat, lon) in COUNTRIES {
        let parts: Vec<String> = name
            .to_ascii_lowercase()
            .split(' ')
            .map(String::from)
            .collect();
        let alias: &[&str] = match *name {
            "England" => &["uk", "britain", "london"],
            "USA" => &["us", "america", "states"],
            _ => &[],
        };
        let matches_name = parts
            .iter()
            .enumerate()
            .all(|(k, p)| words.get(i + k).is_some_and(|w| w == p));
        if matches_name || alias.contains(&words[i]) {
            let used = if matches_name { parts.len() } else { 1 };
            return Some((
                Place {
                    name: (*name).to_string(),
                    lat: *lat,
                    lon: *lon,
                },
                used,
            ));
        }
    }
    None
}

/// Words that stand for what a picture or document is about.
fn concept(w: &str) -> Option<(&'static str, &'static [&'static str])> {
    Some(match w {
        "receipt" | "receipts" => ("a receipt", &["receipt", "total", "subtotal", "vat"]),
        "invoice" | "invoices" | "bill" | "bills" => {
            ("an invoice", &["invoice", "amount due", "bill to"])
        }
        "ticket" | "tickets" => ("a ticket", &["ticket", "boarding", "admit"]),
        "contract" | "contracts" | "agreement" => {
            ("a contract", &["agreement", "contract", "signature"])
        }
        "passport" => ("a passport", &["passport"]),
        "statement" | "statements" => ("a statement", &["statement", "balance"]),
        _ => return None,
    })
}

fn size_value(n: &str, unit: &str) -> Option<u64> {
    let v: f64 = n.replace(',', "").parse().ok()?;
    let mult = match unit {
        "kb" | "k" => 1e3,
        "mb" | "m" | "megs" | "meg" => 1e6,
        "gb" | "g" | "gigs" | "gig" => 1e9,
        "tb" => 1e12,
        _ => return None,
    };
    Some((v * mult) as u64)
}

/// "1gb" or "1.5" "gb" starting at `i`: (bytes, words used).
fn size_at(words: &[&str], i: usize) -> Option<(u64, usize)> {
    let w = words.get(i)?;
    let split = w.find(|c: char| c.is_ascii_alphabetic());
    if let Some(at) = split.filter(|at| *at > 0) {
        return size_value(&w[..at], &w[at..]).map(|b| (b, 1));
    }
    size_value(w, words.get(i + 1)?).map(|b| (b, 2))
}

fn human(bytes: u64) -> String {
    crate::settings::human(bytes)
}

/// Local midnight of the day holding `t`, as unix seconds.
fn day_start(t: i64, off: i64) -> i64 {
    (t + off).div_euclid(86_400) * 86_400 - off
}

fn month_start(y: i64, m: u32, off: i64) -> i64 {
    days_from_civil(y, m, 1) * 86_400 - off
}

const MONTH_NAMES: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

fn short_date(t: i64, off: i64) -> String {
    let (_, m, d) = civil_from_days((t + off).div_euclid(86_400));
    let name = MONTH_NAMES[(m - 1) as usize];
    format!("{d} {}{}", name[..1].to_ascii_uppercase(), &name[1..3])
}

/// A date range starting at word `i`: (from, to, label, words used).
fn when_at(words: &[&str], i: usize, now: i64, off: i64) -> Option<(i64, i64, String, usize)> {
    let today = day_start(now, off);
    let days = (now + off).div_euclid(86_400);
    let (y, m, _) = civil_from_days(days);
    // Monday = 0 (1970-01-01 was a Thursday).
    let weekday = (days + 3).rem_euclid(7);
    let week = today - weekday * 86_400;
    let w = words[i];
    let next = words.get(i + 1).copied().unwrap_or("");
    let range = |a: i64, b: i64, used| {
        let label = if b - a <= 86_400 {
            short_date(a, off)
        } else {
            format!("{}–{}", short_date(a, off), short_date(b - 1, off))
        };
        Some((a, b, label, used))
    };
    match (w, next) {
        ("today", _) => Some((today, today + 86_400, "Today".into(), 1)),
        ("yesterday", _) => Some((today - 86_400, today, "Yesterday".into(), 1)),
        ("this", "week") => range(week, now + 1, 2),
        ("last", "week") => range(week - 7 * 86_400, week, 2),
        ("this", "month") => range(month_start(y, m, off), now + 1, 2),
        ("last", "month") => {
            let (py, pm) = if m == 1 { (y - 1, 12) } else { (y, m - 1) };
            range(month_start(py, pm, off), month_start(y, m, off), 2)
        }
        ("this", "year") => Some((month_start(y, 1, off), now + 1, format!("{y}"), 2)),
        ("last", "year") => Some((
            month_start(y - 1, 1, off),
            month_start(y, 1, off),
            format!("{}", y - 1),
            2,
        )),
        ("recent" | "recently" | "lately", _) => range(today - 30 * 86_400, now + 1, 1),
        ("last" | "past", n) if n.parse::<i64>().is_ok() => {
            let n: i64 = n.parse().ok()?;
            let unit = words.get(i + 2).copied().unwrap_or("days");
            let span = match unit.trim_end_matches('s') {
                "day" => 86_400,
                "week" => 7 * 86_400,
                "month" => 30 * 86_400,
                "year" => 365 * 86_400,
                _ => return None,
            };
            range(today + 86_400 - n * span, now + 1, 3)
        }
        _ => {
            if let Some(mi) = MONTH_NAMES.iter().position(|n| {
                w == *n || (w.len() >= 3 && n.starts_with(w) && w != "mar" && w.len() == 3)
            }) {
                let mm = mi as u32 + 1;
                // "march 2024", else the most recent March.
                let (yy, used) = match next.parse::<i64>() {
                    Ok(yy) if (1990..=2100).contains(&yy) => (yy, 2),
                    _ => (if mm > m { y - 1 } else { y }, 1),
                };
                let end = if mm == 12 {
                    month_start(yy + 1, 1, off)
                } else {
                    month_start(yy, mm + 1, off)
                };
                let name = MONTH_NAMES[mi];
                return Some((
                    month_start(yy, mm, off),
                    end,
                    format!("{}{} {yy}", name[..1].to_ascii_uppercase(), &name[1..]),
                    used,
                ));
            }
            if let Ok(yy) = w.parse::<i64>() {
                if (1990..=2100).contains(&yy) {
                    return Some((
                        month_start(yy, 1, off),
                        month_start(yy + 1, 1, off),
                        format!("{yy}"),
                        1,
                    ));
                }
            }
            None
        }
    }
}

/// Split into lower-case words, keeping money and sizes together.
fn tokens(q: &str) -> Vec<String> {
    q.to_lowercase()
        .split(|c: char| c.is_whitespace() || c == '?' || c == '!' || c == '"')
        .map(|w| w.trim_matches(|c: char| c == ',' || c == '.' || c == '\'' || c == ';'))
        .filter(|w| !w.is_empty())
        .map(String::from)
        .collect()
}

/// Turn what the person typed into what to look for. `drop` lists the
/// `Part::key`s the person took away.
pub fn understand(q: &str, now: i64, tz_offset_minutes: i32, drop: &[String]) -> Understood {
    let off = tz_offset_minutes as i64 * 60;
    let owned = tokens(q);
    let words: Vec<&str> = owned.iter().map(String::as_str).collect();
    let mut u = Understood::default();
    let dropped = |k: &str| drop.iter().any(|d| d == k);
    let mut i = 0;
    while i < words.len() {
        let w = words[i];
        let prev = if i > 0 { words[i - 1] } else { "" };

        if matches!(w, "deleted" | "trash" | "trashed" | "removed" | "lost") {
            u.only_deleted = true;
            i += 1;
            continue;
        }
        if let Some(k) = kind_word(w) {
            if !u.kinds.contains(&k) {
                u.kinds.push(k);
            }
            i += 1;
            continue;
        }
        // "over 1 GB", "bigger than 500mb", "under 2 MB"
        let above = matches!(w, "over" | "above" | "bigger" | "larger" | "more" | ">");
        let below = matches!(w, "under" | "below" | "smaller" | "less" | "<");
        if above || below {
            let skip = if words.get(i + 1) == Some(&"than") {
                2
            } else {
                1
            };
            if let Some((bytes, used)) = size_at(&words, i + skip) {
                if above {
                    u.min_size = Some(bytes);
                } else {
                    u.max_size = Some(bytes);
                }
                i += skip + used;
                continue;
            }
        }
        if let Some((a, b, label, used)) = when_at(&words, i, now, off) {
            u.after = Some(a);
            u.before = Some(b);
            u.parts.push(Part {
                key: "when".into(),
                label: "When".into(),
                value: label,
            });
            i += used;
            continue;
        }
        if matches!(prev, "from" | "in" | "via" | "on" | "at") || u.from.is_none() {
            let two = words.get(i + 1).map(|n| format!("{w} {n}"));
            if let Some((name, pats)) = two.as_deref().and_then(source).or_else(|| {
                matches!(prev, "from" | "via" | "on" | "in")
                    .then(|| source(w))
                    .flatten()
            }) {
                u.from = Some((name.into(), pats.iter().map(|p| p.to_string()).collect()));
                i += if two.as_deref().and_then(source).is_some() {
                    2
                } else {
                    1
                };
                continue;
            }
            if matches!(prev, "from" | "in" | "at") {
                if let Some((place, used)) = country(&words, i) {
                    u.place = Some(place);
                    i += used;
                    continue;
                }
                // "from bbc.co.uk": a web address.
                if w.contains('.')
                    && w.chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
                {
                    u.from = Some((w.to_string(), vec![w.to_string()]));
                    i += 1;
                    continue;
                }
            }
        }
        if let Some((label, alts)) = concept(w) {
            u.words.push(alts.iter().map(|a| a.to_string()).collect());
            u.parts.push(Part {
                key: format!("about:{w}"),
                label: "Shows".into(),
                value: label.into(),
            });
            i += 1;
            continue;
        }
        // Money: "$2,000", "£50", "2000 dollars"
        let money = w.trim_start_matches(['$', '£', '€']);
        if money != w
            || words
                .get(i + 1)
                .is_some_and(|n| matches!(*n, "dollars" | "pounds" | "euros"))
        {
            let digits = money.replace(',', "");
            if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit() || c == '.') {
                let mut alts = vec![digits.clone()];
                if let Ok(n) = digits.parse::<u64>() {
                    if n >= 1000 {
                        alts.push(format!("{},{:03}", n / 1000, n % 1000));
                    }
                }
                u.parts.push(Part {
                    key: format!("text:{digits}"),
                    label: "Mentions".into(),
                    value: w.into(),
                });
                u.words.push(alts);
                i += if money == w { 2 } else { 1 };
                continue;
            }
        }
        if !STOP.contains(&w)
            && w.chars().any(char::is_alphanumeric)
            && !matches!(w, "than" | "last" | "this")
        {
            u.parts.push(Part {
                key: format!("text:{w}"),
                label: "Mentions".into(),
                value: w.into(),
            });
            u.words.push(vec![w.to_string()]);
        }
        i += 1;
    }

    // Kinds and the rest, in the order they read best.
    let mut front = Vec::new();
    if u.only_deleted {
        front.push(Part {
            key: "deleted".into(),
            label: "Only".into(),
            value: "deleted".into(),
        });
    }
    for k in &u.kinds {
        front.push(Part {
            key: format!("kind:{}", k.label().to_ascii_lowercase()),
            label: "Kind".into(),
            value: k.label().into(),
        });
    }
    if let Some((name, _)) = &u.from {
        front.push(Part {
            key: "from".into(),
            label: "Came from".into(),
            value: name.clone(),
        });
    }
    if let Some(p) = &u.place {
        front.push(Part {
            key: "place".into(),
            label: "Taken in".into(),
            value: p.name.clone(),
        });
    }
    if let Some(b) = u.min_size {
        front.push(Part {
            key: "min".into(),
            label: "Over".into(),
            value: human(b),
        });
    }
    if let Some(b) = u.max_size {
        front.push(Part {
            key: "max".into(),
            label: "Under".into(),
            value: human(b),
        });
    }
    front.append(&mut u.parts);
    u.parts = front;

    // What the person dropped.
    if dropped("deleted") {
        u.only_deleted = false;
    }
    u.kinds
        .retain(|k| !dropped(&format!("kind:{}", k.label().to_ascii_lowercase())));
    if dropped("from") {
        u.from = None;
    }
    if dropped("place") {
        u.place = None;
    }
    if dropped("min") {
        u.min_size = None;
    }
    if dropped("max") {
        u.max_size = None;
    }
    if dropped("when") {
        u.after = None;
        u.before = None;
    }
    let keep: Vec<bool> = u
        .parts
        .iter()
        .filter(|p| p.key.starts_with("text:") || p.key.starts_with("about:"))
        .map(|p| !dropped(&p.key))
        .collect();
    let mut k = keep.into_iter();
    u.words.retain(|_| k.next().unwrap_or(true));
    u.parts.retain(|p| !dropped(&p.key));
    u
}

/// Make a value safe inside a Spotlight query string.
fn quote(v: &str) -> String {
    v.chars()
        .filter(|c| !matches!(c, '"' | '\\' | '*' | '\'' | '\n'))
        .collect()
}

fn iso(t: i64) -> String {
    let (y, m, d) = civil_from_days(t.div_euclid(86_400));
    let s = t.rem_euclid(86_400);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        s / 3600,
        s % 3600 / 60,
        s % 60
    )
}

/// The Spotlight query for live files, or None when there is nothing to ask.
pub fn spotlight_query(u: &Understood) -> Option<String> {
    if u.is_empty() || u.only_deleted {
        return None;
    }
    let mut all = Vec::new();
    if !u.kinds.is_empty() {
        let kinds: Vec<String> = u
            .kinds
            .iter()
            .map(|k| format!("kMDItemContentTypeTree == \"{}\"", k.uti()))
            .collect();
        all.push(format!("({})", kinds.join(" || ")));
        if u.kinds == [Kind::Screenshot] {
            all.push("kMDItemIsScreenCapture == 1".into());
        }
    }
    if let Some((_, pats)) = &u.from {
        let any: Vec<String> = pats
            .iter()
            .map(|p| format!("kMDItemWhereFroms == \"*{}*\"cd", quote(p)))
            .collect();
        all.push(format!("({})", any.join(" || ")));
    }
    if let (Some(a), Some(b)) = (u.after, u.before) {
        let (a, b) = (iso(a), iso(b));
        all.push(format!(
            "((kMDItemDateAdded >= $time.iso({a}) && kMDItemDateAdded < $time.iso({b})) || (kMDItemContentCreationDate >= $time.iso({a}) && kMDItemContentCreationDate < $time.iso({b})))"
        ));
    }
    if let Some(n) = u.min_size {
        all.push(format!("kMDItemFSSize > {n}"));
    }
    if let Some(n) = u.max_size {
        all.push(format!("kMDItemFSSize < {n}"));
    }
    if let Some(p) = &u.place {
        all.push(format!(
            "kMDItemLatitude >= {} && kMDItemLatitude <= {} && kMDItemLongitude >= {} && kMDItemLongitude <= {}",
            p.lat.0, p.lat.1, p.lon.0, p.lon.1
        ));
    }
    for group in &u.words {
        let any: Vec<String> = group
            .iter()
            .map(|w| quote(w))
            .filter(|w| !w.is_empty())
            .flat_map(|w| {
                [
                    format!("kMDItemDisplayName == \"*{w}*\"cd"),
                    format!("kMDItemTextContent == \"*{w}*\"cd"),
                ]
            })
            .collect();
        if !any.is_empty() {
            all.push(format!("({})", any.join(" || ")));
        }
    }
    (!all.is_empty()).then(|| all.join(" && "))
}

/// Why a file was deleted, when it was, and how to get it back.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Deleted {
    /// "In the Trash", "Photos · Recently Deleted"…
    pub label: String,
    /// The finding in the last scan, which `export::recover` takes.
    pub finding: Option<u64>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub path: String,
    pub name: String,
    pub media: Media,
    pub size: u64,
    pub time: Option<i64>,
    /// The folder it is in, shortened for display ("Documents › Work").
    pub place: String,
    /// The site a download came from.
    pub came_from: Option<String>,
    pub deleted: Option<Deleted>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    pub parts: Vec<Part>,
    pub hits: Vec<Hit>,
    /// True when Spotlight was asked and answered.
    pub spotlight: bool,
}

fn mtime(meta: &fs::Metadata) -> Option<i64> {
    let d = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    i64::try_from(d.as_secs()).ok()
}

/// "Documents › Work" for `~/Documents/Work/x.pdf`.
pub fn place_label(path: &Path, home: &Path) -> String {
    let parent = path.parent().unwrap_or(path);
    let rel = parent.strip_prefix(home).unwrap_or(parent);
    let parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .filter(|c| c != "/")
        .collect();
    match parts.len() {
        0 => "Home".into(),
        1 | 2 => parts.join(" › "),
        n => format!("{} › … › {}", parts[0], parts[n - 1]),
    }
}

/// The folders Find leaves out by default.
pub fn default_excludes() -> Vec<String> {
    vec!["~/Library".into(), "node_modules".into(), ".Trash".into()]
}

fn excluded(path: &Path, home: &Path, excludes: &[String]) -> bool {
    let s = path.to_string_lossy();
    if s.split('/')
        .any(|c| c.starts_with('.') && c.len() > 1 && c != ".Trash")
        || s.contains(".app/")
    {
        return true;
    }
    excludes.iter().any(|e| {
        if let Some(rest) = e.strip_prefix("~/") {
            path.starts_with(home.join(rest))
        } else if e.starts_with('/') {
            path.starts_with(e)
        } else {
            s.split('/').any(|c| c == e)
        }
    })
}

/// Does a deleted finding or trashed file fit what was understood?
fn fits(u: &Understood, name: &str, text: Option<&str>, size: u64, time: Option<i64>) -> bool {
    if !u.kinds.is_empty() && !u.kinds.iter().any(|k| k.fits(name)) {
        return false;
    }
    if u.min_size.is_some_and(|m| size <= m) || u.max_size.is_some_and(|m| size >= m) {
        return false;
    }
    if let (Some(a), Some(b)) = (u.after, u.before) {
        if !time.is_some_and(|t| t >= a && t < b) {
            return false;
        }
    }
    if u.from.is_some() || u.place.is_some() {
        return false; // Not known for deleted files.
    }
    let hay = format!(
        "{} {}",
        name.to_lowercase(),
        text.unwrap_or("").to_lowercase()
    );
    u.words
        .iter()
        .all(|g| g.iter().any(|w| hay.contains(&w.to_lowercase())))
}

/// Deleted files that fit: findings of the last scan, then the Trash.
pub fn deleted_hits(u: &Understood, report: Option<&Report>, home: &Path) -> Vec<Hit> {
    let mut hits = Vec::new();
    let mut seen = HashSet::new();
    if let Some(r) = report {
        for f in &r.findings {
            if f.status != crate::model::Status::Deleted {
                continue;
            }
            if !fits(u, &f.name, f.text.as_deref(), f.size, f.time) {
                continue;
            }
            if f.path.is_none() || f.size == 0 && f.text.is_none() {
                continue;
            }
            if let Some(p) = &f.path {
                seen.insert(p.clone());
            }
            hits.push(Hit {
                path: f.path.clone().unwrap_or_default(),
                name: f.name.clone(),
                media: f.media,
                size: f.size,
                time: f.time,
                place: f.reason.clone(),
                came_from: None,
                deleted: Some(Deleted {
                    label: match f.source {
                        crate::model::Source::PhotosRecentlyDeleted => {
                            "Photos · Recently Deleted".into()
                        }
                        crate::model::Source::TrashFolder => "In the Trash".into(),
                        _ => "Deleted".into(),
                    },
                    finding: Some(f.id),
                }),
            });
        }
    }
    let trash = home.join(".Trash");
    if let Ok(entries) = fs::read_dir(&trash) {
        for e in entries.filter_map(Result::ok) {
            let path = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            let Ok(meta) = e.metadata() else { continue };
            if !meta.is_file() || meta.len() == 0 || seen.contains(&path.display().to_string()) {
                continue;
            }
            if trashed(&format!(".Trash/{name}")).is_none() {
                continue;
            }
            #[cfg(unix)]
            let moved = {
                use std::os::unix::fs::MetadataExt;
                Some(meta.ctime())
            };
            #[cfg(not(unix))]
            let moved = mtime(&meta);
            if !fits(u, &name, None, meta.len(), moved) {
                continue;
            }
            hits.push(Hit {
                path: path.display().to_string(),
                name: name.clone(),
                media: media_for_name(&name),
                size: meta.len(),
                time: moved,
                place: "In the Trash".into(),
                came_from: None,
                deleted: Some(Deleted {
                    label: "In the Trash".into(),
                    finding: None,
                }),
            });
        }
    }
    hits
}

/// `mdls -name kMDItemWhereFroms a b c` output: one entry per file, in order.
pub fn parse_where_froms(out: &str) -> Vec<Vec<String>> {
    let mut all = Vec::new();
    let mut current: Option<Vec<String>> = None;
    for line in out.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("kMDItemWhereFroms") {
            if let Some(c) = current.take() {
                all.push(c);
            }
            let rest = rest.trim_start_matches([' ', '=']).trim();
            if rest == "(null)" {
                all.push(Vec::new());
            } else {
                current = Some(Vec::new());
            }
            continue;
        }
        if let Some(c) = current.as_mut() {
            if t == ")" {
                all.push(current.take().unwrap_or_default());
            } else {
                let url = t.trim_end_matches(',').trim_matches('"');
                if !url.is_empty() && url != "(" {
                    c.push(url.to_string());
                }
            }
        }
    }
    if let Some(c) = current {
        all.push(c);
    }
    all
}

/// The site to show for a download: the page it was downloaded from.
pub fn host_of(urls: &[String]) -> Option<String> {
    let pick = urls
        .iter()
        .find(|u| u.contains("mail.google.com"))
        .or_else(|| urls.last())?;
    let rest = pick.split("://").nth(1)?;
    let host = rest.split(['/', '?', '#']).next()?;
    (!host.is_empty()).then(|| host.trim_start_matches("www.").to_string())
}

/// Ask Spotlight, reading at most `limit` paths.
fn mdfind(query: &str, home: &Path, limit: usize) -> Option<Vec<PathBuf>> {
    let tool = Path::new("/usr/bin/mdfind");
    if !tool.is_file() {
        return None;
    }
    let mut child = Command::new(tool)
        .arg("-0")
        .arg("-onlyin")
        .arg(home)
        .arg(query)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut out = child.stdout.take()?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    let mut paths = Vec::new();
    loop {
        let n = out.read(&mut chunk).unwrap_or(0);
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        while let Some(at) = buf.iter().position(|b| *b == 0) {
            let p = String::from_utf8_lossy(&buf[..at]).into_owned();
            buf.drain(..=at);
            paths.push(PathBuf::from(p));
        }
        if paths.len() >= limit {
            let _ = child.kill();
            break;
        }
    }
    let _ = child.wait();
    Some(paths)
}

fn came_from_all(paths: &[&Path]) -> Vec<Option<String>> {
    let tool = Path::new("/usr/bin/mdls");
    if paths.is_empty() || !tool.is_file() {
        return vec![None; paths.len()];
    }
    let out = Command::new(tool)
        .args(["-name", "kMDItemWhereFroms"])
        .args(paths)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let parsed = parse_where_froms(&out);
    if parsed.len() != paths.len() {
        return vec![None; paths.len()];
    }
    parsed.iter().map(|u| host_of(u)).collect()
}

/// Everything Find shows for a query.
#[allow(clippy::too_many_arguments)]
pub fn find(
    query: &str,
    drop: &[String],
    report: Option<&Report>,
    home: &Path,
    now: i64,
    tz_offset_minutes: i32,
) -> Found {
    let settings = crate::settings::load(home);
    let u = understand(query, now, tz_offset_minutes, drop);
    let excludes = settings
        .find_exclude
        .clone()
        .unwrap_or_else(default_excludes);
    let with_deleted = settings.find_deleted.unwrap_or(true) || u.only_deleted;
    let mut hits = Vec::new();
    let mut spotlight = false;
    if let Some(q) = spotlight_query(&u) {
        if let Some(paths) = mdfind(&q, home, MAX_RESULTS * 2) {
            spotlight = true;
            for path in paths {
                if hits.len() >= MAX_RESULTS {
                    break;
                }
                if excluded(&path, home, &excludes) {
                    continue;
                }
                let Ok(meta) = fs::metadata(&path) else {
                    continue;
                };
                if !meta.is_file() {
                    continue;
                }
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                hits.push(Hit {
                    place: place_label(&path, home),
                    path: path.display().to_string(),
                    media: media_for_name(&name),
                    name,
                    size: meta.len(),
                    time: mtime(&meta),
                    came_from: None,
                    deleted: None,
                });
            }
        }
    }
    hits.sort_by_key(|h| std::cmp::Reverse(h.time));
    if with_deleted && !u.is_empty() {
        let mut gone = deleted_hits(&u, report, home);
        gone.sort_by_key(|h| std::cmp::Reverse(h.time));
        hits.extend(gone.into_iter().take(40));
    }
    // Where downloads came from, for the first screenful.
    let first: Vec<&Path> = hits
        .iter()
        .filter(|h| h.deleted.is_none())
        .take(40)
        .map(|h| Path::new(&h.path))
        .collect();
    let hosts = came_from_all(&first);
    let mut it = hosts.into_iter();
    for h in hits.iter_mut().filter(|h| h.deleted.is_none()).take(40) {
        h.came_from = it.next().flatten();
    }
    Found {
        parts: u.parts,
        hits,
        spotlight,
    }
}

/// A small picture of a file in the results, as a data URL.
pub fn thumbnail(path: &str, cache: &Path) -> Option<String> {
    use std::hash::{Hash, Hasher};
    let name = Path::new(path).file_name()?.to_string_lossy().into_owned();
    let media = media_for_name(&name);
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h);
    let mut f = crate::model::Finding::new(
        crate::model::Status::Deleted,
        crate::model::Source::TrashFolder,
        media,
        name,
    );
    // Kept apart from scan thumbnails, which are named by finding id.
    f.id = h.finish() >> 12;
    f.path = Some(path.to_string());
    crate::export::thumbnail(&f, &cache.join("find"))
}

/// Copy files (deleted ones from the Trash) to the Save to location.
pub fn recover_paths(
    paths: &[String],
    home: &Path,
    now: i64,
    tz_offset_minutes: i32,
) -> Result<String, String> {
    let files: Vec<PathBuf> = paths
        .iter()
        .map(PathBuf::from)
        .filter(|p| fs::metadata(p).is_ok_and(|m| m.is_file() && m.len() > 0))
        .collect();
    if files.is_empty() {
        return Err("Nothing to recover: the file is gone or empty.".into());
    }
    let needed: u64 = files
        .iter()
        .filter_map(|p| fs::metadata(p).ok())
        .map(|m| m.len())
        .sum();
    let root = crate::settings::save_root(home)?;
    crate::settings::check_space(&root, "what you picked", Some(needed), 0)?;
    let dest =
        crate::devices::backup_folder(&root, "Recovered", now + tz_offset_minutes as i64 * 60);
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    for p in &files {
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let target = crate::export::unique(dest.join(crate::export::clean(&name)));
        fs::copy(p, target).map_err(|e| format!("Could not copy {name}: {e}"))?;
    }
    Ok(dest.display().to_string())
}

/// Find's settings, for its section in the window.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FindSettings {
    pub shortcut: String,
    /// Said beside the shortcut when an installed app may use it too.
    pub warning: Option<String>,
    pub include_deleted: bool,
    pub excludes: Vec<String>,
}

pub fn find_settings(home: &Path, apps: &Path) -> FindSettings {
    let s = crate::settings::load(home);
    let shortcut = s.find_shortcut.unwrap_or_else(|| DEFAULT_SHORTCUT.into());
    FindSettings {
        warning: check_shortcut(&shortcut, apps).ok().flatten(),
        shortcut,
        include_deleted: s.find_deleted.unwrap_or(true),
        excludes: s.find_exclude.unwrap_or_else(default_excludes),
    }
}

/// Change one of Find's settings and save.
pub fn update_find_settings(
    home: &Path,
    change: impl FnOnce(&mut crate::settings::Settings),
) -> Result<(), String> {
    let mut s = crate::settings::load(home);
    change(&mut s);
    crate::settings::save(home, &s)
}

/// Shortcuts macOS keeps for itself, which Find never takes.
const SYSTEM: &[(&str, &str)] = &[
    ("Cmd+Space", "Spotlight"),
    ("Cmd+Alt+Space", "the Finder search window"),
    ("Ctrl+Space", "switching keyboard language"),
    ("Ctrl+Alt+Space", ""),
    ("Ctrl+Cmd+Space", "Emoji & Symbols"),
    ("Cmd+Tab", "switching apps"),
    ("Cmd+Q", "quitting apps"),
    ("Cmd+W", "closing windows"),
    ("Cmd+Shift+3", "screenshots"),
    ("Cmd+Shift+4", "screenshots"),
    ("Cmd+Shift+5", "screenshots"),
];

/// Apps that often take ⌥Space for themselves.
const LAUNCHERS: &[(&str, &str)] = &[
    ("Alfred 5.app", "Alfred"),
    ("Alfred 4.app", "Alfred"),
    ("Raycast.app", "Raycast"),
    ("ChatGPT.app", "ChatGPT"),
];

/// The shortcut in a fixed order of modifiers, e.g. "Cmd+Alt+Space".
pub fn normalize(shortcut: &str) -> String {
    let mut mods = Vec::new();
    let mut key = String::new();
    for p in shortcut.split('+').map(str::trim) {
        match p.to_ascii_lowercase().as_str() {
            "ctrl" | "control" | "⌃" => mods.push(0),
            "alt" | "option" | "opt" | "⌥" => mods.push(1),
            "shift" | "⇧" => mods.push(2),
            "cmd" | "command" | "super" | "meta" | "⌘" => mods.push(3),
            other => {
                key = if other.len() == 1 {
                    other.to_ascii_uppercase()
                } else {
                    let mut c = other.chars();
                    c.next()
                        .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                        .unwrap_or_default()
                }
            }
        }
    }
    mods.sort_unstable();
    mods.dedup();
    let names = ["Ctrl", "Alt", "Shift", "Cmd"];
    let mut parts: Vec<String> = mods.iter().map(|m| names[*m].to_string()).collect();
    // Spotlight writes ⌘⌥Space; keep Cmd before Alt to match SYSTEM.
    if parts.contains(&"Cmd".to_string()) && parts.contains(&"Alt".to_string()) {
        parts.retain(|p| p != "Cmd");
        parts.insert(0, "Cmd".into());
    }
    if parts.contains(&"Ctrl".to_string()) && parts.contains(&"Cmd".to_string()) {
        parts.retain(|p| p != "Ctrl");
        parts.insert(0, "Ctrl".into());
    }
    parts.push(key);
    parts.join("+")
}

/// Whether a shortcut can be used: Err with why not, Ok with a warning
/// when an installed app commonly uses it.
pub fn check_shortcut(shortcut: &str, apps: &Path) -> Result<Option<String>, String> {
    let s = normalize(shortcut);
    if !s.contains('+') {
        return Err("Add at least one of ⌘, ⌥, ⌃ or ⇧, so typing never opens Find.".into());
    }
    if let Some((_, who)) = SYSTEM.iter().find(|(k, who)| *k == s && !who.is_empty()) {
        return Err(format!("macOS uses that for {who}. Choose another."));
    }
    if s == "Alt+Space" {
        let installed: Vec<&str> = LAUNCHERS
            .iter()
            .filter(|(app, _)| apps.join(app).exists())
            .map(|(_, name)| *name)
            .collect();
        if let Some(name) = installed.first() {
            return Ok(Some(format!(
                "{name} is installed and often uses ⌥Space. If Find doesn't open, use ⌃⌥Space instead."
            )));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrity::tests::temp_dir;

    // Tuesday 17 November 2026, 09:41 UTC.
    const NOW: i64 = 1_794_908_460;

    fn u(q: &str) -> Understood {
        understand(q, NOW, 0, &[])
    }

    fn values(u: &Understood) -> Vec<String> {
        u.parts
            .iter()
            .map(|p| format!("{}={}", p.label, p.value))
            .collect()
    }

    #[test]
    fn now_is_a_tuesday() {
        let days = NOW.div_euclid(86_400);
        assert_eq!(civil_from_days(days), (2026, 11, 17));
        assert_eq!((days + 3).rem_euclid(7), 1, "Tuesday is 1");
    }

    #[test]
    fn pdfs_from_gmail_last_week() {
        let x = u("pdfs from gmail last week");
        assert_eq!(x.kinds, vec![Kind::Pdf]);
        assert_eq!(x.from.as_ref().unwrap().0, "Gmail");
        // Last week: Monday 9 to Monday 16 November.
        assert_eq!(x.after, Some(days_from_civil(2026, 11, 9) * 86_400));
        assert_eq!(x.before, Some(days_from_civil(2026, 11, 16) * 86_400));
        assert!(x.words.is_empty());
        assert_eq!(
            values(&x),
            vec!["Kind=PDF", "Came from=Gmail", "When=9 Nov–15 Nov"]
        );
        let q = spotlight_query(&x).unwrap();
        assert!(
            q.contains("kMDItemContentTypeTree == \"com.adobe.pdf\""),
            "{q}"
        );
        assert!(
            q.contains("kMDItemWhereFroms == \"*mail.google.com*\"cd"),
            "{q}"
        );
        assert!(q.contains("$time.iso(2026-11-09T00:00:00Z)"), "{q}");
    }

    #[test]
    fn videos_over_a_gigabyte() {
        for q in [
            "videos over 1 GB",
            "videos over 1gb",
            "videos bigger than 1 gb",
        ] {
            let x = u(q);
            assert_eq!(x.kinds, vec![Kind::Video], "{q}");
            assert_eq!(x.min_size, Some(1_000_000_000), "{q}");
            assert!(x.words.is_empty(), "{q}: {:?}", x.words);
        }
        assert!(spotlight_query(&u("videos over 1 GB"))
            .unwrap()
            .contains("kMDItemFSSize > 1000000000"));
    }

    #[test]
    fn photos_from_greece() {
        let x = u("photos from Greece");
        assert_eq!(x.kinds, vec![Kind::Photo]);
        assert_eq!(x.place.as_ref().unwrap().name, "Greece");
        assert!(x.from.is_none());
        assert!(spotlight_query(&x)
            .unwrap()
            .contains("kMDItemLatitude >= 34.8"));
    }

    #[test]
    fn photo_of_a_receipt_and_an_invoice_amount() {
        let x = u("photo of a receipt");
        assert_eq!(x.kinds, vec![Kind::Photo]);
        assert_eq!(values(&x), vec!["Kind=Photo", "Shows=a receipt"]);
        assert!(x.words[0].contains(&"total".to_string()));

        let y = u("invoice for $2,000");
        assert_eq!(y.words.len(), 2);
        assert!(y.words[1].contains(&"2000".to_string()));
        assert!(y.words[1].contains(&"2,000".to_string()));
    }

    #[test]
    fn dropping_a_chip_removes_it() {
        let x = understand(
            "pdfs from gmail last week",
            NOW,
            0,
            &["from".into(), "when".into()],
        );
        assert_eq!(values(&x), vec!["Kind=PDF"]);
        assert!(x.from.is_none() && x.after.is_none());
        let y = understand("lease agreement", NOW, 0, &["text:lease".into()]);
        assert_eq!(y.words.len(), 1);
    }

    #[test]
    fn dates_months_and_plain_words() {
        let x = u("tax return march 2024");
        assert_eq!(x.after, Some(days_from_civil(2024, 3, 1) * 86_400));
        assert_eq!(x.before, Some(days_from_civil(2024, 4, 1) * 86_400));
        assert_eq!(
            x.words,
            vec![vec!["tax".to_string()], vec!["return".to_string()]]
        );
        // The most recent December was last year.
        let d = u("photos december");
        assert_eq!(d.after, Some(days_from_civil(2025, 12, 1) * 86_400));
        let t = u("deleted yesterday");
        assert!(t.only_deleted);
        assert_eq!(
            spotlight_query(&t),
            None,
            "deleted-only never asks Spotlight"
        );
        assert_eq!(spotlight_query(&u("")), None);
    }

    #[test]
    fn spotlight_query_values_cannot_break_out() {
        let x = u("report\"*) || kMDItemFSSize > 0");
        let q = spotlight_query(&x).unwrap();
        assert!(!q.contains("report\""), "{q}");
    }

    #[test]
    fn reads_where_froms() {
        let out = "kMDItemWhereFroms = (\n    \"https://mail-attachment.googleusercontent.com/x\",\n    \"https://mail.google.com/\"\n)\nkMDItemWhereFroms = (null)\nkMDItemWhereFroms = (\n    \"https://www.bbc.co.uk/news\"\n)\n";
        let p = parse_where_froms(out);
        assert_eq!(p.len(), 3);
        assert_eq!(host_of(&p[0]).as_deref(), Some("mail.google.com"));
        assert_eq!(host_of(&p[1]), None);
        assert_eq!(host_of(&p[2]).as_deref(), Some("bbc.co.uk"));
    }

    #[test]
    fn finds_deleted_files_in_the_trash() {
        let home = temp_dir("find-trash");
        fs::create_dir_all(home.join(".Trash")).unwrap();
        fs::write(home.join(".Trash/Statement October.pdf"), b"%PDF-1.4 data").unwrap();
        fs::write(home.join(".Trash/holiday.jpg"), b"jpeg").unwrap();
        fs::write(home.join(".Trash/empty.pdf"), b"").unwrap();
        let hits = deleted_hits(&u("statement pdf"), None, &home);
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert_eq!(hits[0].name, "Statement October.pdf");
        assert_eq!(hits[0].deleted.as_ref().unwrap().label, "In the Trash");
        assert!(
            deleted_hits(&u("pdfs from gmail"), None, &home).is_empty(),
            "origin unknown for deleted files"
        );

        let saved = recover_paths(&[hits[0].path.clone()], &home, NOW, 0).unwrap();
        assert!(Path::new(&saved).join("Statement October.pdf").is_file());
    }

    #[test]
    fn places_and_excludes() {
        let home = Path::new("/Users/me");
        assert_eq!(
            place_label(Path::new("/Users/me/Documents/Work/a.pdf"), home),
            "Documents › Work"
        );
        assert_eq!(place_label(Path::new("/Users/me/a.pdf"), home), "Home");
        let ex = default_excludes();
        assert!(excluded(
            Path::new("/Users/me/Library/Caches/x.pdf"),
            home,
            &ex
        ));
        assert!(excluded(
            Path::new("/Users/me/code/node_modules/a/x.pdf"),
            home,
            &ex
        ));
        assert!(excluded(
            Path::new("/Users/me/Apps/Thing.app/Contents/x.pdf"),
            home,
            &ex
        ));
        assert!(!excluded(Path::new("/Users/me/Downloads/x.pdf"), home, &ex));
    }

    #[test]
    fn settings_round_trip_and_keep_the_save_location() {
        let home = temp_dir("find-settings");
        let apps = temp_dir("find-settings-apps");
        crate::settings::set_save_to(&home, Some(home.to_str().unwrap())).unwrap();
        assert_eq!(find_settings(&home, &apps).shortcut, DEFAULT_SHORTCUT);
        update_find_settings(&home, |s| {
            s.find_shortcut = Some("Ctrl+Alt+Space".into());
            s.find_deleted = Some(false);
        })
        .unwrap();
        let f = find_settings(&home, &apps);
        assert_eq!(f.shortcut, "Ctrl+Alt+Space");
        assert!(!f.include_deleted);
        assert!(crate::settings::load(&home).save_to.is_some());
    }

    #[test]
    fn shortcuts_never_clash_with_spotlight() {
        let apps = temp_dir("apps");
        assert_eq!(normalize("option+space"), "Alt+Space");
        assert_eq!(normalize("Alt+Cmd+Space"), "Cmd+Alt+Space");
        assert!(check_shortcut("Cmd+Space", &apps)
            .unwrap_err()
            .contains("Spotlight"));
        assert!(check_shortcut("Cmd+Alt+Space", &apps)
            .unwrap_err()
            .contains("Finder"));
        assert!(check_shortcut("Ctrl+Space", &apps).is_err());
        assert!(check_shortcut("Space", &apps).is_err());
        assert_eq!(check_shortcut(DEFAULT_SHORTCUT, &apps), Ok(None));
        assert_eq!(check_shortcut("Ctrl+Alt+Space", &apps), Ok(None));
        fs::create_dir_all(apps.join("Raycast.app")).unwrap();
        assert!(check_shortcut("Alt+Space", &apps)
            .unwrap()
            .unwrap()
            .contains("Raycast"));
    }
}
