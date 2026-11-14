//! Searching, filtering and grouping findings on a timeline, from years down
//! to single minutes.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::model::{Finding, Media, Source, Status};
use crate::paths::app_display_name;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Zoom {
    Year,
    Month,
    Day,
    Hour,
    Minute,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Query {
    /// Words that must all appear (in the name, path, app, reason or text).
    pub text: String,
    pub app: Option<String>,
    pub status: Option<Status>,
    pub media: Option<Media>,
    pub zoom: Zoom,
    /// Unix seconds, inclusive. Undated findings are left out when set.
    pub from: Option<i64>,
    /// Unix seconds, exclusive.
    pub to: Option<i64>,
    /// Minutes east of UTC, so days and hours match the user's clock.
    pub tz_offset_minutes: i32,
    pub limit: usize,
}

impl Default for Query {
    fn default() -> Self {
        Query {
            text: String::new(),
            app: None,
            status: None,
            media: None,
            zoom: Zoom::Year,
            from: None,
            to: None,
            tz_offset_minutes: 0,
            limit: 500,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Bucket {
    pub start: i64,
    pub end: i64,
    pub label: String,
    pub count: usize,
    /// How many of `count` are damaged rather than deleted.
    pub corrupt: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppCount {
    pub id: String,
    pub name: String,
    pub count: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub buckets: Vec<Bucket>,
    /// Newest first, undated last, at most `limit`.
    pub items: Vec<Finding>,
    /// How many findings match, before `limit`.
    pub total: usize,
    pub undated: usize,
    pub apps: Vec<AppCount>,
    pub deleted: usize,
    pub corrupt: usize,
    /// Matching findings by kind of content and by where they were found.
    pub by_media: BTreeMap<Media, usize>,
    pub by_source: BTreeMap<Source, usize>,
}

/// Days since 1970-01-01 for a proleptic Gregorian date.
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// (year, month, day) for days since 1970-01-01.
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (
        if m <= 2 {
            yoe + era * 400 + 1
        } else {
            yoe + era * 400
        },
        m,
        d,
    )
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// The bucket holding local time `local`: (start, end) in local seconds.
fn bucket_bounds(local: i64, zoom: Zoom) -> (i64, i64) {
    let days = local.div_euclid(86_400);
    let (y, m, _) = civil_from_days(days);
    match zoom {
        Zoom::Year => (
            days_from_civil(y, 1, 1) * 86_400,
            days_from_civil(y + 1, 1, 1) * 86_400,
        ),
        Zoom::Month => {
            let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
            (
                days_from_civil(y, m, 1) * 86_400,
                days_from_civil(ny, nm, 1) * 86_400,
            )
        }
        Zoom::Day => (days * 86_400, (days + 1) * 86_400),
        Zoom::Hour => {
            let s = local.div_euclid(3_600) * 3_600;
            (s, s + 3_600)
        }
        Zoom::Minute => {
            let s = local.div_euclid(60) * 60;
            (s, s + 60)
        }
    }
}

fn label(local_start: i64, zoom: Zoom) -> String {
    let days = local_start.div_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    let secs = local_start.rem_euclid(86_400);
    let (h, min) = (secs / 3_600, secs % 3_600 / 60);
    let month = MONTHS[(m - 1) as usize];
    match zoom {
        Zoom::Year => y.to_string(),
        Zoom::Month => format!("{month} {y}"),
        Zoom::Day => format!(
            "{} {d} {month} {y}",
            WEEKDAYS[(days + 4).rem_euclid(7) as usize]
        ),
        Zoom::Hour => format!("{h:02}:00"),
        Zoom::Minute => format!("{h:02}:{min:02}"),
    }
}

fn matches_text(f: &Finding, words: &[String]) -> bool {
    if words.is_empty() {
        return true;
    }
    let mut hay = String::new();
    for part in [
        Some(f.name.as_str()),
        Some(f.reason.as_str()),
        f.path.as_deref(),
        f.original_path.as_deref(),
        f.app.as_deref(),
        f.text.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        hay.push_str(&part.to_lowercase());
        hay.push('\n');
    }
    if let Some(app) = &f.app {
        hay.push_str(&app_display_name(app).to_lowercase());
    }
    words.iter().all(|w| hay.contains(w.as_str()))
}

pub fn query(findings: &[Finding], q: &Query) -> QueryResult {
    let offset = q.tz_offset_minutes as i64 * 60;
    let words: Vec<String> = q.text.split_whitespace().map(str::to_lowercase).collect();

    // Everything except the app filter and time range: drives the app list.
    let base: Vec<&Finding> = findings
        .iter()
        .filter(|f| q.status.is_none_or(|s| f.status == s))
        .filter(|f| q.media.is_none_or(|m| f.media == m))
        .filter(|f| matches_text(f, &words))
        .collect();

    let mut app_counts: HashMap<&str, usize> = HashMap::new();
    for f in &base {
        if let Some(app) = &f.app {
            *app_counts.entry(app.as_str()).or_default() += 1;
        }
    }
    let mut apps: Vec<AppCount> = app_counts
        .into_iter()
        .map(|(id, count)| AppCount {
            id: id.to_string(),
            name: app_display_name(id),
            count,
        })
        .collect();
    apps.sort_by(|a, b| b.count.cmp(&a.count).then(a.name.cmp(&b.name)));

    let in_app: Vec<&Finding> = base
        .into_iter()
        .filter(|f| q.app.as_ref().is_none_or(|a| f.app.as_ref() == Some(a)))
        .collect();

    let ranged = q.from.is_some() || q.to.is_some();
    let mut selected: Vec<&Finding> = in_app
        .iter()
        .copied()
        .filter(|f| match f.time {
            Some(t) => q.from.is_none_or(|from| t >= from) && q.to.is_none_or(|to| t < to),
            None => !ranged,
        })
        .collect();

    let mut buckets: Vec<Bucket> = Vec::new();
    let mut index: HashMap<i64, usize> = HashMap::new();
    for f in &selected {
        let Some(t) = f.time else { continue };
        let (start, end) = bucket_bounds(t + offset, q.zoom);
        let i = *index.entry(start).or_insert_with(|| {
            buckets.push(Bucket {
                start: start - offset,
                end: end - offset,
                label: label(start, q.zoom),
                count: 0,
                corrupt: 0,
            });
            buckets.len() - 1
        });
        buckets[i].count += 1;
        if f.status == Status::Corrupt {
            buckets[i].corrupt += 1;
        }
    }
    buckets.sort_by_key(|b| b.start);

    selected.sort_by(|a, b| match (a.time, b.time) {
        (Some(x), Some(y)) => y.cmp(&x),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.id.cmp(&b.id),
    });
    let mut by_media = BTreeMap::new();
    let mut by_source = BTreeMap::new();
    for f in &selected {
        *by_media.entry(f.media).or_insert(0) += 1;
        *by_source.entry(f.source).or_insert(0) += 1;
    }
    let total = selected.len();
    let undated = selected.iter().filter(|f| f.time.is_none()).count();
    let deleted = selected
        .iter()
        .filter(|f| f.status == Status::Deleted)
        .count();
    QueryResult {
        buckets,
        items: selected.into_iter().take(q.limit.max(1)).cloned().collect(),
        total,
        undated,
        apps,
        deleted,
        corrupt: total - deleted,
        by_media,
        by_source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Source;

    fn finding(id: u64, time: Option<i64>, app: Option<&str>, name: &str) -> Finding {
        let mut f = Finding::new(
            Status::Deleted,
            Source::TrashFolder,
            Media::Image,
            name.into(),
        );
        f.id = id;
        f.time = time;
        f.app = app.map(String::from);
        f
    }

    #[test]
    fn civil_round_trip() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2024, 2, 29), 19_782);
        for z in [-1000, 0, 19_782, 30_000] {
            let (y, m, d) = civil_from_days(z);
            assert_eq!(days_from_civil(y, m, d), z);
        }
    }

    #[test]
    fn buckets_follow_zoom_and_time_zone() {
        // 2024-03-04 23:30 UTC; in UTC+1 that is 2024-03-05 00:30.
        let t = days_from_civil(2024, 3, 4) * 86_400 + 23 * 3_600 + 30 * 60;
        let fs = vec![finding(1, Some(t), None, "a")];
        let mut q = Query {
            zoom: Zoom::Day,
            ..Query::default()
        };
        assert_eq!(query(&fs, &q).buckets[0].label, "Mon 4 Mar 2024");
        q.tz_offset_minutes = 60;
        let r = query(&fs, &q);
        assert_eq!(r.buckets[0].label, "Tue 5 Mar 2024");
        assert_eq!(r.buckets[0].end - r.buckets[0].start, 86_400);
        q.zoom = Zoom::Minute;
        assert_eq!(query(&fs, &q).buckets[0].label, "00:30");
        q.zoom = Zoom::Month;
        assert_eq!(query(&fs, &q).buckets[0].label, "Mar 2024");
    }

    #[test]
    fn filters_by_app_text_and_range() {
        let fs = vec![
            finding(1, Some(100), Some("com.whatsapp"), "chat.jpg"),
            finding(2, Some(200), Some("Photos"), "beach.heic"),
            finding(3, None, Some("com.whatsapp"), "voice.opus"),
        ];
        let q = Query {
            app: Some("com.whatsapp".into()),
            ..Query::default()
        };
        let r = query(&fs, &q);
        assert_eq!(r.total, 2);
        assert_eq!(r.by_media.get(&Media::Image), Some(&2));
        assert_eq!(r.by_source.get(&Source::TrashFolder), Some(&2));
        assert_eq!(r.undated, 1);
        assert_eq!(r.items[0].id, 1); // dated first
        assert_eq!(r.apps.len(), 2); // app list ignores the app filter

        let q = Query {
            text: "WHATSAPP voice".into(),
            ..Query::default()
        };
        assert_eq!(query(&fs, &q).total, 1);

        let q = Query {
            from: Some(150),
            ..Query::default()
        };
        let r = query(&fs, &q);
        assert_eq!(r.items.iter().map(|f| f.id).collect::<Vec<_>>(), vec![2]);
    }
}
