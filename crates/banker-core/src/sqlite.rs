//! A small, read-only SQLite reader, and recovery of deleted records.
//!
//! Almost every app on a phone keeps its data (messages, call logs, notes,
//! photo libraries) in SQLite. When a row is deleted SQLite usually leaves
//! its bytes in place and only marks the space free, in three ways:
//!
//! * freeblocks inside a page, where the deleted cell's bytes stay put;
//! * the unallocated gap between a page's cell pointers and its cells;
//! * whole pages moved to the freelist.
//!
//! On top of that, the write-ahead log (`-wal` file) keeps earlier versions
//! of pages, so a row deleted since the last checkpoint is still in there.
//!
//! This module reads all of those and nothing else: live rows are only read
//! to tell old text from current text, never reported.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::carve::carve_pictures;
use crate::integrity::Kind;
use crate::model::Source;
use crate::util::{be_u16, be_u32, text_runs};

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Int(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl Value {
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            Value::Real(r) => Some(*r as i64),
            _ => None,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Real(r) => Some(*r),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Text(s) => Some(s),
            _ => None,
        }
    }
}

/// Read a SQLite varint. Returns the value and the number of bytes used.
pub fn varint(data: &[u8], at: usize) -> Option<(i64, usize)> {
    let mut value: u64 = 0;
    for i in 0..9 {
        let byte = *data.get(at + i)?;
        if i == 8 {
            value = (value << 8) | byte as u64;
            return Some((value as i64, 9));
        }
        value = (value << 7) | (byte & 0x7F) as u64;
        if byte & 0x80 == 0 {
            return Some((value as i64, i + 1));
        }
    }
    None
}

fn serial_size(serial: i64) -> Option<usize> {
    Some(match serial {
        0 | 8 | 9 => 0,
        1 => 1,
        2 => 2,
        3 => 3,
        4 => 4,
        5 => 6,
        6 | 7 => 8,
        10 | 11 => return None,
        n if n >= 12 => ((n - 12) / 2) as usize,
        _ => return None,
    })
}

fn read_value(serial: i64, bytes: &[u8]) -> Value {
    let int = |n: usize| {
        let mut v: i64 = if bytes[0] & 0x80 != 0 { -1 } else { 0 };
        for &b in &bytes[..n] {
            v = (v << 8) | b as i64;
        }
        v
    };
    match serial {
        0 => Value::Null,
        1 => Value::Int(int(1)),
        2 => Value::Int(int(2)),
        3 => Value::Int(int(3)),
        4 => Value::Int(int(4)),
        5 => Value::Int(int(6)),
        6 => Value::Int(int(8)),
        7 => Value::Real(f64::from_bits(u64::from_be_bytes(
            bytes[..8].try_into().unwrap_or([0; 8]),
        ))),
        8 => Value::Int(0),
        9 => Value::Int(1),
        n if n >= 13 && n % 2 == 1 => Value::Text(String::from_utf8_lossy(bytes).into_owned()),
        _ => Value::Blob(bytes.to_vec()),
    }
}

/// Decode a record whose header starts at `at`. Returns the values and the
/// offset just past the record, or None if the bytes are not a record.
pub fn decode_record_at(data: &[u8], at: usize) -> Option<(Vec<Value>, usize)> {
    let (header_len, n) = varint(data, at)?;
    let header_len = usize::try_from(header_len).ok()?;
    if header_len < n + 1 || header_len > 512 {
        return None;
    }
    let header_end = at.checked_add(header_len)?;
    if header_end > data.len() {
        return None;
    }
    let mut serials = Vec::new();
    let mut pos = at + n;
    while pos < header_end {
        let (serial, used) = varint(data, pos)?;
        serial_size(serial)?;
        serials.push(serial);
        pos += used;
    }
    if pos != header_end || serials.is_empty() {
        return None;
    }
    let mut values = Vec::with_capacity(serials.len());
    let mut body = header_end;
    for serial in serials {
        let size = serial_size(serial)?;
        let bytes = data.get(body..body.checked_add(size)?)?;
        values.push(read_value(serial, if size == 0 { &[0] } else { bytes }));
        body += size;
    }
    Some((values, body))
}

/// Column names from a CREATE TABLE statement, in order.
pub fn column_names(sql: &str) -> Vec<String> {
    let (Some(open), Some(close)) = (sql.find('('), sql.rfind(')')) else {
        return Vec::new();
    };
    if close <= open {
        return Vec::new();
    }
    let mut columns = Vec::new();
    let mut depth = 0;
    let mut current = String::new();
    for c in sql[open + 1..close].chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                columns.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    columns.push(current);
    columns
        .iter()
        .filter_map(|def| {
            let first = def.split_whitespace().next()?;
            let name = first.trim_matches(|c| matches!(c, '"' | '`' | '[' | ']' | '\''));
            let upper = name.to_ascii_uppercase();
            let constraint = matches!(
                upper.as_str(),
                "PRIMARY" | "UNIQUE" | "CHECK" | "FOREIGN" | "CONSTRAINT"
            );
            (!constraint && !name.is_empty()).then(|| name.to_string())
        })
        .collect()
}

pub struct Table {
    pub name: String,
    pub root: usize,
    pub columns: Vec<String>,
}

pub struct Db {
    data: Vec<u8>,
    page_size: usize,
    usable: usize,
    page_count: usize,
    /// Pages replaced by newer versions (from the WAL), by page number.
    overrides: HashMap<usize, Vec<u8>>,
}

pub const HEADER: &[u8] = b"SQLite format 3\0";

impl Db {
    pub fn parse(data: Vec<u8>) -> Result<Db, &'static str> {
        if !data.starts_with(HEADER) || data.len() < 512 {
            return Err("not a SQLite database");
        }
        let page_size = match be_u16(&data, 16).ok_or("short header")? {
            1 => 65_536,
            n if n >= 512 && n.is_power_of_two() => n as usize,
            _ => return Err("bad page size"),
        };
        let reserved = data[20] as usize;
        let page_count = data.len() / page_size;
        Ok(Db {
            usable: page_size - reserved,
            page_size,
            page_count,
            data,
            overrides: HashMap::new(),
        })
    }

    pub fn page_size(&self) -> usize {
        self.page_size
    }

    /// The current version of page `n` (1-based).
    pub fn page(&self, n: usize) -> Option<&[u8]> {
        if let Some(p) = self.overrides.get(&n) {
            return Some(p);
        }
        if n == 0 || n > self.page_count {
            return None;
        }
        self.data.get((n - 1) * self.page_size..n * self.page_size)
    }

    /// The version of page `n` in the main file, ignoring the WAL.
    fn file_page(&self, n: usize) -> Option<&[u8]> {
        if n == 0 || n > self.page_count {
            return None;
        }
        self.data.get((n - 1) * self.page_size..n * self.page_size)
    }

    fn max_page(&self) -> usize {
        self.overrides
            .keys()
            .copied()
            .max()
            .unwrap_or(0)
            .max(self.page_count)
    }

    fn header_offset(n: usize) -> usize {
        if n == 1 {
            100
        } else {
            0
        }
    }

    /// The payload of a table-leaf cell, following overflow pages.
    fn payload(&self, page: &[u8], at: usize, len: usize) -> Option<Vec<u8>> {
        let u = self.usable;
        let x = u - 35;
        if len <= x {
            return page.get(at..at + len).map(<[u8]>::to_vec);
        }
        let m = ((u - 12) * 32 / 255) - 23;
        let k = m + ((len - m) % (u - 4));
        let local = if k <= x { k } else { m };
        let mut out = page.get(at..at + local)?.to_vec();
        let mut next = be_u32(page, at + local)? as usize;
        let mut hops = 0;
        while out.len() < len && next != 0 {
            let p = self.page(next)?;
            let take = (len - out.len()).min(u - 4);
            out.extend_from_slice(p.get(4..4 + take)?);
            next = be_u32(p, 0)? as usize;
            hops += 1;
            if hops > self.max_page() {
                return None;
            }
        }
        (out.len() == len).then_some(out)
    }

    /// Every page of the b-tree rooted at `root`, in visiting order.
    pub fn btree_pages(&self, root: usize) -> Vec<usize> {
        let mut pages = Vec::new();
        let mut seen = HashSet::new();
        let mut stack = vec![root];
        while let Some(n) = stack.pop() {
            if !seen.insert(n) {
                continue;
            }
            let Some(page) = self.page(n) else { continue };
            let h = Self::header_offset(n);
            let Some(&kind) = page.get(h) else { continue };
            if !matches!(kind, 0x02 | 0x05 | 0x0A | 0x0D) {
                continue;
            }
            pages.push(n);
            if kind == 0x02 || kind == 0x05 {
                let cells = be_u16(page, h + 3).unwrap_or(0) as usize;
                for i in 0..cells {
                    let Some(ptr) = be_u16(page, h + 12 + i * 2) else {
                        break;
                    };
                    if let Some(child) = be_u32(page, ptr as usize) {
                        stack.push(child as usize);
                    }
                }
                if let Some(right) = be_u32(page, h + 8) {
                    stack.push(right as usize);
                }
            }
        }
        pages
    }

    /// Live rows of the table b-tree rooted at `root`.
    pub fn rows(&self, root: usize) -> Vec<(i64, Vec<Value>)> {
        let mut rows = Vec::new();
        for n in self.btree_pages(root) {
            let Some(page) = self.page(n) else { continue };
            let h = Self::header_offset(n);
            if page[h] != 0x0D {
                continue;
            }
            let cells = be_u16(page, h + 3).unwrap_or(0) as usize;
            for i in 0..cells {
                let Some(ptr) = be_u16(page, h + 8 + i * 2) else {
                    break;
                };
                let ptr = ptr as usize;
                let Some((len, a)) = varint(page, ptr) else {
                    continue;
                };
                let Some((rowid, b)) = varint(page, ptr + a) else {
                    continue;
                };
                let Ok(len) = usize::try_from(len) else {
                    continue;
                };
                if let Some(payload) = self.payload(page, ptr + a + b, len) {
                    if let Some((values, _)) = decode_record_at(&payload, 0) {
                        rows.push((rowid, values));
                    }
                }
            }
        }
        rows
    }

    /// Tables listed in the schema.
    pub fn tables(&self) -> Vec<Table> {
        self.rows(1)
            .into_iter()
            .filter_map(|(_, v)| {
                if v.first()?.as_str()? != "table" {
                    return None;
                }
                Some(Table {
                    name: v.get(1)?.as_str()?.to_string(),
                    root: usize::try_from(v.get(3)?.as_i64()?).ok()?,
                    columns: column_names(v.get(4).and_then(Value::as_str).unwrap_or("")),
                })
            })
            .collect()
    }

    pub fn table(&self, name: &str) -> Option<Table> {
        self.tables()
            .into_iter()
            .find(|t| t.name.eq_ignore_ascii_case(name))
    }

    /// Live rows of a table as maps from column name to value.
    pub fn table_rows(&self, name: &str) -> Vec<HashMap<String, Value>> {
        let Some(table) = self.table(name) else {
            return Vec::new();
        };
        self.rows(table.root)
            .into_iter()
            .map(|(rowid, values)| {
                let mut row: HashMap<String, Value> = table
                    .columns
                    .iter()
                    .cloned()
                    .zip(values)
                    .map(|(c, v)| (c.to_ascii_uppercase(), v))
                    .collect();
                // An INTEGER PRIMARY KEY column is stored as NULL and lives in the rowid.
                if let Some(first) = table.columns.first() {
                    let key = first.to_ascii_uppercase();
                    if matches!(row.get(&key), Some(Value::Null)) {
                        row.insert(key, Value::Int(rowid));
                    }
                }
                row
            })
            .collect()
    }

    /// Page numbers on the freelist.
    fn freelist(&self) -> Vec<usize> {
        let mut pages = Vec::new();
        let mut trunk = be_u32(&self.data, 32).unwrap_or(0) as usize;
        let mut seen = HashSet::new();
        while trunk != 0 && seen.insert(trunk) {
            let Some(page) = self.page(trunk) else { break };
            pages.push(trunk);
            let count = be_u32(page, 4).unwrap_or(0) as usize;
            for i in 0..count.min((self.usable - 8) / 4) {
                if let Some(leaf) = be_u32(page, 8 + i * 4) {
                    pages.push(leaf as usize);
                }
            }
            trunk = be_u32(page, 0).unwrap_or(0) as usize;
        }
        pages
    }

    /// Apply a write-ahead log. Returns earlier versions of pages, which hold
    /// rows deleted or changed since the last checkpoint.
    pub fn apply_wal(&mut self, wal: &[u8]) -> Vec<(usize, Vec<u8>)> {
        let magic = be_u32(wal, 0).unwrap_or(0);
        if magic != 0x377F_0682 && magic != 0x377F_0683 {
            return Vec::new();
        }
        let page_size = be_u32(wal, 8).unwrap_or(0) as usize;
        if page_size != self.page_size {
            return Vec::new();
        }
        let salts = (be_u32(wal, 16), be_u32(wal, 20));
        let mut history = Vec::new();
        let mut latest: BTreeMap<usize, Vec<u8>> = BTreeMap::new();
        let mut at = 32;
        while at + 24 + page_size <= wal.len() {
            let n = be_u32(wal, at).unwrap_or(0) as usize;
            let current = (be_u32(wal, at + 8), be_u32(wal, at + 12)) == salts;
            let page = wal[at + 24..at + 24 + page_size].to_vec();
            if n != 0 {
                if current {
                    if let Some(old) = latest.insert(n, page) {
                        history.push((n, old));
                    }
                } else {
                    history.push((n, page));
                }
            }
            at += 24 + page_size;
        }
        for (n, page) in latest {
            if let Some(old) = self.file_page(n) {
                history.push((n, old.to_vec()));
            }
            self.overrides.insert(n, page);
        }
        history
    }
}

/// One piece of deleted content recovered from a database.
#[derive(Clone, Debug, PartialEq)]
pub struct Fragment {
    pub source: Source,
    pub table: Option<String>,
    pub text: String,
    pub time: Option<i64>,
    pub time_estimated: bool,
}

/// A picture rebuilt from free space in a database.
#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    pub kind: Kind,
    pub bytes: Vec<u8>,
    pub table: Option<String>,
}

#[derive(Default)]
pub struct Recovery {
    pub fragments: Vec<Fragment>,
    pub pictures: Vec<Picture>,
}

/// Turn a number into a Unix time if it plausibly is one, trying the epochs
/// and units apps actually use. `now` bounds the range.
pub fn plausible_time(value: f64, now: i64) -> Option<i64> {
    const MAC_EPOCH: f64 = 978_307_200.0; // 2001-01-01, Core Data and Cocoa
    const WIN_EPOCH: f64 = 11_644_473_600.0; // 1601-01-01, Chrome and WebKit
    let lo = 1_199_145_600.0; // 2008-01-01
    let hi = now as f64 + 366.0 * 86_400.0;
    let candidates = [
        value,
        value / 1e3,
        value / 1e6,
        value + MAC_EPOCH,
        value / 1e9 + MAC_EPOCH,
        value / 1e6 - WIN_EPOCH,
    ];
    candidates
        .into_iter()
        .find(|t| t.is_finite() && *t >= lo && *t <= hi)
        .map(|t| t as i64)
}

const MAX_FRAGMENTS: usize = 5_000;
/// Text runs found outside any decodable record must be at least this long.
const MIN_TEXT: usize = 6;

/// Loose text must read like words, not like binary that happens to be
/// printable: at least three letters, and mostly letters, digits and spaces.
fn meaningful(text: &str) -> bool {
    let total = text.chars().count();
    let letters = text.chars().filter(|c| c.is_alphabetic()).count();
    let wordy = text
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .count();
    letters >= 3 && wordy * 10 >= total * 8
}

struct Collector<'a> {
    now: i64,
    live: &'a HashSet<String>,
    seen: HashSet<String>,
    out: Recovery,
}

impl Collector<'_> {
    fn keep(&self, text: &str) -> bool {
        !self.live.contains(text) && !self.seen.contains(text)
    }

    fn push(&mut self, f: Fragment) {
        if self.out.fragments.len() < MAX_FRAGMENTS && self.keep(&f.text) {
            self.seen.insert(f.text.clone());
            self.out.fragments.push(f);
        }
    }

    fn record(&mut self, values: &[Value], table: Option<&Table>, source: Source) -> bool {
        let texts: Vec<&str> = values
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|t| t.chars().count() >= 2 && t.chars().any(char::is_alphanumeric))
            .collect();
        if texts.is_empty() || texts.iter().all(|t| self.live.contains(*t)) {
            return false;
        }
        let mut time = None;
        let mut estimated = true;
        if let Some(table) = table {
            for (column, value) in table.columns.iter().zip(values) {
                let name = column.to_ascii_lowercase();
                if name.contains("date") || name.contains("time") {
                    if let Some(t) = value.as_f64().and_then(|v| plausible_time(v, self.now)) {
                        time = Some(t);
                        estimated = false;
                        break;
                    }
                }
            }
        }
        if time.is_none() {
            time = values
                .iter()
                .filter_map(Value::as_f64)
                .find_map(|v| plausible_time(v, self.now));
        }
        self.push(Fragment {
            source,
            table: table.map(|t| t.name.clone()),
            text: texts.join(" · "),
            time,
            time_estimated: estimated,
        });
        true
    }

    fn loose_text(&mut self, region: &[u8], table: Option<&Table>, source: Source) {
        for (_, text) in text_runs(region, MIN_TEXT) {
            if self.keep(&text) && meaningful(&text) && !looks_like_schema(&text) {
                self.push(Fragment {
                    source,
                    table: table.map(|t| t.name.clone()),
                    text,
                    time: None,
                    time_estimated: true,
                });
            }
        }
    }

    /// Free space usually still holds whole cells: deleted rows, and stale
    /// copies of rows that moved when a page was rebalanced. Decode every
    /// cell that fits exactly; `record` drops the ones still live. Only the
    /// bytes no cell explains are searched for loose text.
    fn cells(&mut self, region: &[u8], table: Option<&Table>, source: Source) {
        let mut loose_from = 0;
        let mut i = 0;
        while i + 3 < region.len() {
            let found = self.cell_at(region, i, table, source).or_else(|| {
                let (values, end) = clobbered_cell(region, i, table, false)?;
                self.record(&values, table, source);
                Some(end)
            });
            match found {
                Some(end) => {
                    if i > loose_from {
                        self.loose_text(&region[loose_from..i], table, source);
                    }
                    i = end;
                    loose_from = end;
                }
                None => i += 1,
            }
        }
        if loose_from < region.len() {
            self.loose_text(&region[loose_from..], table, source);
        }
        self.pictures(region, table);
    }

    /// A table cell (length, rowid, record) or index cell (length, record)
    /// starting at `at` whose record ends exactly where its length says.
    fn cell_at(
        &mut self,
        region: &[u8],
        at: usize,
        table: Option<&Table>,
        source: Source,
    ) -> Option<usize> {
        let (len, a) = varint(region, at)?;
        let len = usize::try_from(len)
            .ok()
            .filter(|l| *l >= 3 && *l <= region.len())?;
        let widths = table.map_or(64, |t| t.columns.len());
        let try_record = |start: usize| {
            decode_record_at(region, start)
                .filter(|(v, end)| *end == start + len && v.len() <= widths)
        };
        let decoded = varint(region, at + a)
            .and_then(|(_, b)| try_record(at + a + b))
            .or_else(|| try_record(at + a))?;
        let (values, end) = decoded;
        if !clean(&values) && values.iter().any(|v| matches!(v, Value::Text(_))) {
            return None;
        }
        self.record(&values, table, source);
        Some(end)
    }

    fn pictures(&mut self, region: &[u8], table: Option<&Table>) {
        for (kind, start, end) in carve_pictures(region) {
            self.out.pictures.push(Picture {
                kind,
                bytes: region[start..end].to_vec(),
                table: table.map(|t| t.name.clone()),
            });
        }
    }

    /// A freeblock keeps the deleted cell except its first four bytes, which
    /// held the payload length, the rowid and often the start of the record
    /// header. Neighbouring freed cells merge into one freeblock, and each
    /// lost its first four bytes when it was freed, so walk them in turn.
    fn freeblock(&mut self, block: &[u8], table: Option<&Table>) {
        let mut pos = 0;
        while pos + 4 < block.len() {
            let Some((values, end)) = clobbered_cell(block, pos, table, true) else {
                break;
            };
            self.record(&values, table, Source::DatabaseFragment);
            pos = end;
        }
        if let Some(rest) = block.get(pos.max(4)..) {
            self.cells(rest, table, Source::DatabaseFragment);
        }
    }

    /// An earlier version of a page, from the WAL. Leaf pages are decoded
    /// cell by cell; rows still current are skipped by `record`.
    fn history_page(&mut self, page: &[u8], table: Option<&Table>) {
        if page.first() == Some(&0x0D) {
            let cells = be_u16(page, 3).unwrap_or(0) as usize;
            for i in 0..cells {
                let Some(ptr) = be_u16(page, 8 + i * 2).map(usize::from) else {
                    break;
                };
                let Some((_, a)) = varint(page, ptr) else {
                    continue;
                };
                let Some((_, b)) = varint(page, ptr + a) else {
                    continue;
                };
                if let Some((values, _)) = decode_record_at(page, ptr + a + b) {
                    self.record(&values, table, Source::DatabaseHistory);
                }
            }
            let (blocks, _) = free_regions(page, 0);
            for block in blocks {
                self.freeblock(block, table);
            }
            self.pictures(page, table);
        } else {
            self.cells(page, table, Source::DatabaseHistory);
        }
    }
}

/// Decode a freed cell starting at `pos` whose first four bytes were
/// overwritten by a freeblock header (next pointer, size). With the table's
/// schema the record header is rebuilt: the lost leading columns are almost
/// always an INTEGER PRIMARY KEY, stored as NULL. Without one (only when
/// `guess` is set), decoding resumes at a header that survived.
///
/// Every freed cell keeps the size SQLite wrote when freeing it: its own
/// length, or more if it was merged with the free space after it. A decode
/// shorter than that size is only believed if the next cell decodes too.
fn clobbered_cell(
    block: &[u8],
    pos: usize,
    table: Option<&Table>,
    guess: bool,
) -> Option<(Vec<Value>, usize)> {
    clobbered_inner(block, pos, table, guess, true)
}

/// `check_size` is off for the one-cell lookahead, which keeps the search
/// linear instead of following a whole chain of merged cells recursively.
fn clobbered_inner(
    block: &[u8],
    pos: usize,
    table: Option<&Table>,
    guess: bool,
    check_size: bool,
) -> Option<(Vec<Value>, usize)> {
    let size = be_u16(block, pos + 2)? as usize;
    if size < 8 {
        return None;
    }
    let mut candidates = Vec::new();
    if let Some(t) = table {
        let k = t.columns.len();
        for at in pos + 4..=pos + 6 {
            for lost in 0..=2.min(k) {
                candidates.extend(decode_with_schema(block, at, k, lost));
            }
        }
    }
    if guess || table.is_none() {
        for skip in pos + 2..=pos + 6 {
            if let Some((values, end)) = decode_record_at(block, skip) {
                if end > skip + 2 && table.is_none_or(|t| values.len() <= t.columns.len() + 1) {
                    candidates.push((values, end));
                }
            }
        }
    }
    candidates.into_iter().find(|(values, end)| {
        let len = end - pos;
        let exact = size >= len && size <= len + 3;
        // The first cell of a merged run carries the size of the whole run;
        // believe that only if the next cell decodes too.
        let merged = || {
            size > len + 3
                && (block.len() - end <= 3
                    || clobbered_inner(block, *end, table, false, false).is_some())
        };
        *end <= block.len() && clean(values) && (!check_size || exact || merged())
    })
}

/// A recovered record is believable only if it has text and all of its text
/// is printable: random bytes decode to control characters quickly.
fn clean(values: &[Value]) -> bool {
    let mut has_text = false;
    for v in values {
        if let Value::Text(t) = v {
            if t.contains('\u{FFFD}')
                || t.chars()
                    .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
            {
                return false;
            }
            has_text |= !t.is_empty();
        }
    }
    has_text
}

/// Decode a record whose first `lost` serial types were overwritten,
/// assuming they were NULL, reading the remaining `columns - lost` types
/// from `at`.
fn decode_with_schema(
    data: &[u8],
    at: usize,
    columns: usize,
    lost: usize,
) -> Option<(Vec<Value>, usize)> {
    let mut serials = vec![0i64; lost];
    let mut pos = at;
    for _ in lost..columns {
        let (serial, used) = varint(data, pos)?;
        serial_size(serial)?;
        serials.push(serial);
        pos += used;
    }
    let mut values = Vec::with_capacity(columns);
    for serial in serials {
        let size = serial_size(serial)?;
        let bytes = data.get(pos..pos.checked_add(size)?)?;
        values.push(read_value(serial, if size == 0 { &[0] } else { bytes }));
        pos += size;
    }
    Some((values, pos))
}

/// SQL and schema names show up in free space after schema changes; they are
/// not user content.
fn looks_like_schema(text: &str) -> bool {
    let upper = text.to_ascii_uppercase();
    [
        "CREATE TABLE",
        "CREATE INDEX",
        "CREATE TRIGGER",
        "INTEGER PRIMARY",
        "SQLITE_",
    ]
    .iter()
    .any(|k| upper.contains(k))
}

/// Free regions of a b-tree leaf page: freeblocks and the unallocated gap.
fn free_regions(page: &[u8], h: usize) -> (Vec<&[u8]>, Option<&[u8]>) {
    let mut blocks = Vec::new();
    let leaf = matches!(page.get(h), Some(0x0D | 0x0A));
    if !leaf {
        return (blocks, None);
    }
    let cells = be_u16(page, h + 3).unwrap_or(0) as usize;
    let content = match be_u16(page, h + 5).unwrap_or(0) {
        0 => 65_536,
        n => n as usize,
    };
    let array_end = h + 8 + cells * 2;
    let gap = (array_end < content && content <= page.len()).then(|| &page[array_end..content]);
    let mut next = be_u16(page, h + 1).unwrap_or(0) as usize;
    let mut hops = 0;
    while next != 0 && next + 4 <= page.len() && hops < 1024 {
        let size = be_u16(page, next + 2).unwrap_or(0) as usize;
        if size < 4 || next + size > page.len() {
            break;
        }
        blocks.push(&page[next..next + size]);
        next = be_u16(page, next).unwrap_or(0) as usize;
        hops += 1;
    }
    (blocks, gap)
}

/// Every text value and text run in the live database, so old text that is
/// also current is not reported as deleted.
fn live_text(db: &Db, tables: &[Table]) -> HashSet<String> {
    let mut live = HashSet::new();
    for table in tables {
        for (_, values) in db.rows(table.root) {
            for v in values {
                if let Value::Text(s) = v {
                    live.insert(s.trim().to_string());
                }
            }
        }
    }
    live
}

/// Recover deleted content from a database file and its optional WAL.
pub fn recover(data: Vec<u8>, wal: Option<&[u8]>, now: i64) -> Result<Recovery, &'static str> {
    let mut db = Db::parse(data)?;
    let history = wal.map(|w| db.apply_wal(w)).unwrap_or_default();
    let tables = db.tables();
    let live = live_text(&db, &tables);

    // Which table owns each page, so findings can say where they came from.
    let mut owner: HashMap<usize, usize> = HashMap::new();
    for (i, t) in tables.iter().enumerate() {
        for p in db.btree_pages(t.root) {
            owner.insert(p, i);
        }
    }

    let mut c = Collector {
        now,
        live: &live,
        seen: HashSet::new(),
        out: Recovery::default(),
    };

    let mut owned: Vec<_> = owner.iter().map(|(p, t)| (*p, *t)).collect();
    owned.sort_unstable();
    for (n, t) in owned {
        let Some(page) = db.page(n) else { continue };
        let table = tables.get(t);
        let (blocks, gap) = free_regions(page, Db::header_offset(n));
        for block in blocks {
            c.freeblock(block, table);
        }
        if let Some(gap) = gap {
            c.cells(gap, table, Source::DatabaseFragment);
        }
    }

    for n in db.freelist() {
        if let Some(page) = db.page(n) {
            c.cells(page, None, Source::DatabaseFragment);
        }
    }

    for (n, page) in &history {
        let table = owner.get(n).and_then(|t| tables.get(*t));
        if *n == 1 {
            c.cells(&page[100..], table, Source::DatabaseHistory);
        } else {
            c.history_page(page, table);
        }
    }

    Ok(c.out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn varints() {
        assert_eq!(varint(&[0x05], 0), Some((5, 1)));
        assert_eq!(varint(&[0x81, 0x00], 0), Some((128, 2)));
        assert_eq!(varint(&[0xFF; 9], 0), Some((-1, 9)));
    }

    #[test]
    fn records_decode() {
        // header: len 4, types: int8 (1), text len 2 (17), NULL (0)
        let data = [4, 1, 17, 0, 42, b'h', b'i'];
        let (values, end) = decode_record_at(&data, 0).unwrap();
        assert_eq!(
            values,
            vec![Value::Int(42), Value::Text("hi".into()), Value::Null]
        );
        assert_eq!(end, 7);
    }

    #[test]
    fn column_names_parse() {
        let sql = "CREATE TABLE message (ROWID INTEGER PRIMARY KEY, \"text\" TEXT, date INTEGER DEFAULT (0), PRIMARY KEY (x), CHECK (a > 0))";
        assert_eq!(column_names(sql), vec!["ROWID", "text", "date"]);
    }

    #[test]
    fn times_in_common_epochs() {
        let now = 1_760_000_000;
        assert_eq!(plausible_time(1_700_000_000.0, now), Some(1_700_000_000));
        assert_eq!(
            plausible_time(1_700_000_000_123.0, now),
            Some(1_700_000_000)
        );
        assert_eq!(
            plausible_time(721_692_800.0, now),
            Some(721_692_800 + 978_307_200)
        );
        assert_eq!(plausible_time(42.0, now), None);
    }

    /// Build a one-table database by hand: page 1 is the schema, page 2 a
    /// table leaf with one live row and one deleted row left in a freeblock,
    /// page 3 a freelist trunk page holding old text.
    pub fn tiny_db() -> Vec<u8> {
        const PS: usize = 1024;
        let mut db = vec![0u8; PS * 3];
        db[..16].copy_from_slice(HEADER);
        db[16..18].copy_from_slice(&(PS as u16).to_be_bytes());
        db[18] = 1;
        db[19] = 1;
        db[21] = 64;
        db[22] = 32;
        db[23] = 32;
        db[28..32].copy_from_slice(&3u32.to_be_bytes());
        db[32..36].copy_from_slice(&3u32.to_be_bytes()); // freelist trunk
        db[36..40].copy_from_slice(&1u32.to_be_bytes());
        db[44..48].copy_from_slice(&4u32.to_be_bytes());
        db[56..60].copy_from_slice(&1u32.to_be_bytes());

        let sql = b"CREATE TABLE msg (id INTEGER PRIMARY KEY, body TEXT, date INTEGER)";
        let mut rec = Vec::new();
        let types = [
            (13 + 2 * 5) as u8,
            (13 + 2 * 3) as u8,
            (13 + 2 * 3) as u8,
            1u8,
        ];
        let sql_type = 13 + 2 * sql.len();
        let mut header = vec![0u8];
        header.extend_from_slice(&types);
        header.extend_from_slice(&[0x80 | (sql_type >> 7) as u8, (sql_type & 0x7F) as u8]);
        header[0] = header.len() as u8;
        rec.extend_from_slice(&header);
        rec.extend_from_slice(b"table");
        rec.extend_from_slice(b"msg");
        rec.extend_from_slice(b"msg");
        rec.push(2);
        rec.extend_from_slice(sql);
        let mut cell = Vec::new();
        cell.extend_from_slice(&[0x80 | (rec.len() >> 7) as u8, (rec.len() & 0x7F) as u8]);
        cell.push(1);
        cell.extend_from_slice(&rec);
        let at = PS - cell.len();
        db[at..PS].copy_from_slice(&cell);
        db[100] = 0x0D;
        db[103..105].copy_from_slice(&1u16.to_be_bytes());
        db[105..107].copy_from_slice(&(at as u16).to_be_bytes());
        db[108..110].copy_from_slice(&(at as u16).to_be_bytes());

        // Page 2: live row "kept message", deleted row in a freeblock.
        let p2 = PS;
        let row = |id: u8, body: &[u8], date: u32| {
            let mut r = vec![4u8, 0, (13 + 2 * body.len()) as u8, 4];
            r.extend_from_slice(body);
            r.extend_from_slice(&date.to_be_bytes());
            let mut c = vec![r.len() as u8, id];
            c.extend_from_slice(&r);
            c
        };
        let live = row(1, b"kept message", 1_700_000_000);
        let dead = row(2, b"secret plans for friday", 1_700_086_400);
        let live_at = PS - live.len();
        let dead_at = live_at - dead.len();
        db[p2 + live_at..p2 + PS].copy_from_slice(&live);
        db[p2 + dead_at..p2 + live_at].copy_from_slice(&dead);
        // Turn the dead cell into a freeblock: next=0, size.
        db[p2 + dead_at..p2 + dead_at + 2].copy_from_slice(&0u16.to_be_bytes());
        db[p2 + dead_at + 2..p2 + dead_at + 4].copy_from_slice(&(dead.len() as u16).to_be_bytes());
        db[p2] = 0x0D;
        db[p2 + 1..p2 + 3].copy_from_slice(&(dead_at as u16).to_be_bytes());
        db[p2 + 3..p2 + 5].copy_from_slice(&1u16.to_be_bytes());
        db[p2 + 5..p2 + 7].copy_from_slice(&(dead_at as u16).to_be_bytes());
        db[p2 + 8..p2 + 10].copy_from_slice(&(live_at as u16).to_be_bytes());

        // Page 3: freelist trunk with leftover text after the header.
        let p3 = 2 * PS;
        db[p3 + 100..p3 + 100 + 25].copy_from_slice(b"old note about the garden");
        db
    }

    #[test]
    fn reads_schema_and_live_rows() {
        let db = Db::parse(tiny_db()).unwrap();
        let tables = db.tables();
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].columns, vec!["id", "body", "date"]);
        let rows = db.table_rows("msg");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["BODY"], Value::Text("kept message".into()));
        assert_eq!(rows[0]["ID"], Value::Int(1));
    }

    #[test]
    fn recovers_deleted_rows_but_not_live_ones() {
        let r = recover(tiny_db(), None, 1_760_000_000).unwrap();
        let texts: Vec<_> = r.fragments.iter().map(|f| f.text.as_str()).collect();
        assert!(texts.contains(&"secret plans for friday"), "{texts:?}");
        assert!(texts.contains(&"old note about the garden"), "{texts:?}");
        assert!(!texts.iter().any(|t| t.contains("kept message")));
        let row = r
            .fragments
            .iter()
            .find(|f| f.text == "secret plans for friday")
            .unwrap();
        assert_eq!(row.table.as_deref(), Some("msg"));
        assert_eq!(row.time, Some(1_700_086_400));
        assert!(!row.time_estimated);
    }

    #[test]
    fn wal_history_surfaces_overwritten_text() {
        let db = tiny_db();
        let ps = 1024;
        // A WAL whose only frame replaces page 2 with an empty leaf: the
        // live row in the main file becomes history.
        let mut wal = vec![0u8; 32];
        wal[..4].copy_from_slice(&0x377F_0682u32.to_be_bytes());
        wal[8..12].copy_from_slice(&(ps as u32).to_be_bytes());
        wal[16..24].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        let mut frame = vec![0u8; 24];
        frame[..4].copy_from_slice(&2u32.to_be_bytes());
        frame[8..16].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        let mut page = vec![0u8; ps];
        page[0] = 0x0D;
        page[5..7].copy_from_slice(&(ps as u16).to_be_bytes());
        frame.extend_from_slice(&page);
        wal.extend_from_slice(&frame);
        let r = recover(db, Some(&wal), 1_760_000_000).unwrap();
        let hit = r
            .fragments
            .iter()
            .find(|f| f.text == "kept message")
            .expect("history text");
        assert_eq!(hit.source, Source::DatabaseHistory);
    }
}
