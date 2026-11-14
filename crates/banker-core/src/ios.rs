//! iPhone backups, as written by Finder or `idevicebackup2`.
//!
//! A backup stores every file under the SHA-1 of its domain and path, in
//! `<first two hex digits>/<hash>`. `Manifest.db` maps hashes back to the
//! app (domain) and the path on the phone, with metadata in a binary plist.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::sqlite::{Db, Value};
use crate::util::be_u64;

pub const MAC_EPOCH: i64 = 978_307_200;

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub file_id: String,
    pub domain: String,
    pub relative_path: String,
    pub modified: Option<i64>,
    pub size: Option<u64>,
}

impl Entry {
    pub fn stored_at(&self, root: &Path) -> PathBuf {
        root.join(self.file_id.get(..2).unwrap_or(""))
            .join(&self.file_id)
    }
}

/// The folder holding `Manifest.db`: the given folder, or a single backup
/// folder inside it (idevicebackup2 writes `<dest>/<UDID>/`).
pub fn backup_root(folder: &Path) -> Option<PathBuf> {
    if folder.join("Manifest.db").is_file() {
        return Some(folder.to_path_buf());
    }
    std::fs::read_dir(folder)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.join("Manifest.db").is_file())
}

pub enum Manifest {
    Entries(Vec<Entry>),
    Encrypted,
    Unreadable,
}

pub fn read_manifest(root: &Path) -> Manifest {
    let Ok(data) = std::fs::read(root.join("Manifest.db")) else {
        return Manifest::Unreadable;
    };
    if !data.starts_with(crate::sqlite::HEADER) {
        return Manifest::Encrypted;
    }
    let Ok(db) = Db::parse(data) else {
        return Manifest::Unreadable;
    };
    let entries = db
        .table_rows("Files")
        .into_iter()
        .filter_map(|row| {
            // flags: 1 file, 2 directory, 4 symlink
            if row.get("FLAGS")?.as_i64()? != 1 {
                return None;
            }
            let (modified, size) = match row.get("FILE") {
                Some(Value::Blob(plist)) => file_metadata(plist),
                _ => (None, None),
            };
            Some(Entry {
                file_id: row.get("FILEID")?.as_str()?.to_string(),
                domain: row.get("DOMAIN")?.as_str()?.to_string(),
                relative_path: row.get("RELATIVEPATH")?.as_str()?.to_string(),
                modified,
                size,
            })
        })
        .collect();
    Manifest::Entries(entries)
}

/// A photo or video in the "Recently Deleted" album.
#[derive(Debug, Clone, PartialEq)]
pub struct TrashedAsset {
    /// e.g. `DCIM/100APPLE`
    pub directory: String,
    pub filename: String,
    /// The asset's id, which names the previews Photos keeps of it.
    pub uuid: Option<String>,
    /// The name it had when it was imported (`IMG_1234.HEIC`), when known.
    /// On a Mac, `filename` is the asset's id instead.
    pub original_name: Option<String>,
    pub trashed_at: Option<i64>,
    pub created_at: Option<i64>,
    pub video: bool,
}

/// Assets marked as trashed in a Photos library database (iOS or macOS).
pub fn trashed_assets(db: &Db) -> Vec<TrashedAsset> {
    let mut rows = db.table_rows("ZASSET");
    if rows.is_empty() {
        rows = db.table_rows("ZGENERICASSET");
    }
    let date = |row: &HashMap<String, Value>, key: &str| {
        row.get(key)
            .and_then(Value::as_f64)
            .filter(|v| *v > 0.0)
            .map(|v| v as i64 + MAC_EPOCH)
    };
    // ZADDITIONALASSETATTRIBUTES.ZASSET points at ZASSET.Z_PK.
    let names: HashMap<i64, String> = db
        .table_rows("ZADDITIONALASSETATTRIBUTES")
        .into_iter()
        .filter_map(|r| {
            let asset = r.get("ZASSET")?.as_i64()?;
            let name = r.get("ZORIGINALFILENAME")?.as_str()?.to_string();
            (!name.is_empty()).then_some((asset, name))
        })
        .collect();
    rows.into_iter()
        .filter(|row| row.get("ZTRASHEDSTATE").and_then(Value::as_i64) == Some(1))
        .filter_map(|row| {
            Some(TrashedAsset {
                directory: row
                    .get("ZDIRECTORY")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                filename: row.get("ZFILENAME")?.as_str()?.to_string(),
                uuid: row.get("ZUUID").and_then(Value::as_str).map(str::to_string),
                original_name: row
                    .get("Z_PK")
                    .and_then(Value::as_i64)
                    .and_then(|pk| names.get(&pk).cloned()),
                trashed_at: date(&row, "ZTRASHEDDATE"),
                created_at: date(&row, "ZDATECREATED"),
                video: row.get("ZKIND").and_then(Value::as_i64) == Some(1),
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
enum Obj {
    Int(i64),
    Other,
    Str(String),
    Dict(Vec<(usize, usize)>),
}

/// LastModified and Size from an MBFile binary plist (NSKeyedArchiver).
fn file_metadata(plist: &[u8]) -> (Option<i64>, Option<u64>) {
    let Some(objects) = parse_bplist(plist) else {
        return (None, None);
    };
    let mut modified = None;
    let mut size = None;
    for obj in &objects {
        let Obj::Dict(pairs) = obj else { continue };
        for (k, v) in pairs {
            let (Some(Obj::Str(key)), Some(Obj::Int(value))) = (objects.get(*k), objects.get(*v))
            else {
                continue;
            };
            match key.as_str() {
                "LastModified" => modified = Some(*value),
                "Size" => size = u64::try_from(*value).ok(),
                _ => {}
            }
        }
    }
    (modified, size)
}

fn parse_bplist(data: &[u8]) -> Option<Vec<Obj>> {
    if !data.starts_with(b"bplist00") || data.len() < 40 {
        return None;
    }
    let trailer = &data[data.len() - 32..];
    let offset_size = trailer[6] as usize;
    let ref_size = trailer[7] as usize;
    let count = usize::try_from(be_u64(trailer, 8)?).ok()?;
    let table = usize::try_from(be_u64(trailer, 24)?).ok()?;
    if count > 100_000 || !(1..=8).contains(&offset_size) || !(1..=8).contains(&ref_size) {
        return None;
    }
    let uint = |at: usize, n: usize| -> Option<usize> {
        let bytes = data.get(at..at + n)?;
        Some(bytes.iter().fold(0usize, |acc, b| (acc << 8) | *b as usize))
    };
    let mut objects = Vec::with_capacity(count);
    for i in 0..count {
        let at = uint(table + i * offset_size, offset_size)?;
        let marker = *data.get(at)?;
        let (kind, info) = (marker >> 4, (marker & 0x0F) as usize);
        let length = |at: usize| -> Option<(usize, usize)> {
            if info != 0x0F {
                return Some((info, at + 1));
            }
            let m = *data.get(at + 1)?;
            let n = 1usize << (m & 0x0F);
            Some((uint(at + 2, n)?, at + 2 + n))
        };
        objects.push(match kind {
            0x1 => {
                let n = 1usize << info;
                let bytes = data.get(at + 1..at + 1 + n)?;
                Obj::Int(bytes.iter().fold(0i64, |acc, b| (acc << 8) | *b as i64))
            }
            0x5 => {
                let (len, start) = length(at)?;
                Obj::Str(String::from_utf8_lossy(data.get(start..start + len)?).into_owned())
            }
            0xD => {
                let (len, start) = length(at)?;
                let mut pairs = Vec::with_capacity(len);
                for j in 0..len {
                    let k = uint(start + j * ref_size, ref_size)?;
                    let v = uint(start + (len + j) * ref_size, ref_size)?;
                    pairs.push((k, v));
                }
                Obj::Dict(pairs)
            }
            _ => Obj::Other,
        });
    }
    Some(objects)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// bplist00 { "LastModified": 1700000000, "Size": 42 }
    fn tiny_plist() -> Vec<u8> {
        let mut p = b"bplist00".to_vec();
        let mut offsets = Vec::new();
        offsets.push(p.len());
        p.extend_from_slice(&[0xD2, 1, 2, 3, 4]); // dict, 2 pairs
        offsets.push(p.len());
        p.push(0x5C);
        p.extend_from_slice(b"LastModified");
        offsets.push(p.len());
        p.push(0x54);
        p.extend_from_slice(b"Size");
        offsets.push(p.len());
        p.push(0x12);
        p.extend_from_slice(&1_700_000_000u32.to_be_bytes());
        offsets.push(p.len());
        p.extend_from_slice(&[0x10, 42]);
        let table = p.len();
        for o in &offsets {
            p.push(*o as u8);
        }
        let mut trailer = vec![0u8; 6];
        trailer.push(1);
        trailer.push(1);
        trailer.extend_from_slice(&(offsets.len() as u64).to_be_bytes());
        trailer.extend_from_slice(&0u64.to_be_bytes());
        trailer.extend_from_slice(&(table as u64).to_be_bytes());
        p.extend_from_slice(&trailer);
        p
    }

    #[test]
    fn reads_mbfile_metadata() {
        assert_eq!(
            file_metadata(&tiny_plist()),
            (Some(1_700_000_000), Some(42))
        );
        assert_eq!(file_metadata(b"not a plist"), (None, None));
    }

    #[test]
    fn stored_path_uses_hash_prefix() {
        let e = Entry {
            file_id: "abcdef".into(),
            domain: String::new(),
            relative_path: String::new(),
            modified: None,
            size: None,
        };
        assert_eq!(e.stored_at(Path::new("/b")), PathBuf::from("/b/ab/abcdef"));
    }
}
