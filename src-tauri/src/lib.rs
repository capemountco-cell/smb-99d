//! The Tauri shell.
//!
//! No logic lives here. A command deserialises its arguments, calls one
//! function in the core crate, and returns the result. A decision made in a
//! command handler is a decision that cannot be tested on Linux.
//!
//! Scans are slow, so commands that touch the disk run on a blocking thread
//! and the window stays responsive. Progress arrives as `scan-progress`
//! events.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use banker_core::clone::{CloneEntry, CloneOptions};
use banker_core::devices::{Device, Discovery};
use banker_core::export::Exported;
use banker_core::preview::{CardPreview, Preview};
use banker_core::scan::Progress;
use banker_core::settings::Location;
use banker_core::timeline::{Query, QueryResult};
use banker_core::transfer::{Group, Item, Selection, Transferred};
use banker_core::Report;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Default)]
struct Session {
    report: Arc<Mutex<Option<Report>>>,
    cancel: Arc<AtomicBool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanSummary {
    root: String,
    files_checked: u64,
    databases_checked: u64,
    findings: usize,
    notes: Vec<String>,
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn work_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_cache_dir().map_err(|e| e.to_string())
}

async fn blocking<T: Send + 'static>(
    job: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(job)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn discover() -> Result<Discovery, String> {
    blocking(|| Ok(banker_core::devices::discover(&home()))).await
}

#[tauri::command]
async fn scan(
    app: AppHandle,
    session: State<'_, Session>,
    device: Device,
    tz_offset_minutes: i32,
) -> Result<ScanSummary, String> {
    let work = work_dir(&app)?;
    let cancel = session.cancel.clone();
    let store = session.report.clone();
    cancel.store(false, Ordering::Relaxed);
    blocking(move || {
        let mut progress = |p: &Progress| {
            let _ = app.emit("scan-progress", p);
        };
        let report = banker_core::scan::scan_device(
            &device,
            &home(),
            now(),
            tz_offset_minutes,
            &work,
            &cancel,
            &mut progress,
        )?;
        let summary = ScanSummary {
            root: report.root.clone(),
            files_checked: report.files_checked,
            databases_checked: report.databases_checked,
            findings: report.findings.len(),
            notes: report.notes.clone(),
        };
        *store.lock().map_err(|e| e.to_string())? = Some(report);
        Ok(summary)
    })
    .await
}

#[tauri::command]
async fn clone_device(
    app: AppHandle,
    session: State<'_, Session>,
    device: Device,
    options: CloneOptions,
    tz_offset_minutes: i32,
) -> Result<CloneEntry, String> {
    let cancel = session.cancel.clone();
    cancel.store(false, Ordering::Relaxed);
    blocking(move || {
        let mut progress = |p: &Progress| {
            let _ = app.emit("scan-progress", p);
        };
        banker_core::clone::clone_device(
            &device,
            &home(),
            now(),
            tz_offset_minutes,
            &options,
            &cancel,
            &mut progress,
        )
    })
    .await
}

#[tauri::command]
async fn list_clones() -> Result<Vec<CloneEntry>, String> {
    blocking(|| Ok(banker_core::clone::list_clones(&home()))).await
}

#[tauri::command]
async fn restore_clone(
    app: AppHandle,
    session: State<'_, Session>,
    folder: String,
    device: Device,
    password: Option<String>,
) -> Result<String, String> {
    let cancel = session.cancel.clone();
    cancel.store(false, Ordering::Relaxed);
    blocking(move || {
        let mut progress = |p: &Progress| {
            let _ = app.emit("scan-progress", p);
        };
        let clone = banker_core::clone::clone_folder(&home(), &folder)?;
        banker_core::clone::restore(&clone, &device, password.as_deref(), &cancel, &mut progress)
    })
    .await
}

#[tauri::command]
async fn clone_groups(folder: String) -> Result<Vec<Group>, String> {
    blocking(move || {
        let clone = banker_core::clone::clone_folder(&home(), &folder)?;
        Ok(banker_core::transfer::groups(&clone))
    })
    .await
}

#[tauri::command]
async fn clone_items(
    folder: String,
    group: String,
    offset: usize,
    limit: usize,
) -> Result<(Vec<Item>, usize), String> {
    blocking(move || {
        let clone = banker_core::clone::clone_folder(&home(), &folder)?;
        banker_core::transfer::items(&clone, &group, offset, limit)
    })
    .await
}

#[tauri::command]
async fn clone_thumbnail(
    app: AppHandle,
    folder: String,
    path: String,
) -> Result<Option<String>, String> {
    let cache = work_dir(&app)?.join("clone-thumbnails");
    blocking(move || {
        let clone = banker_core::clone::clone_folder(&home(), &folder)?;
        Ok(banker_core::transfer::thumbnail(&clone, &path, &cache))
    })
    .await
}

#[tauri::command]
async fn transfer_clone(
    app: AppHandle,
    session: State<'_, Session>,
    folder: String,
    device: Device,
    selection: Selection,
    album: String,
) -> Result<Transferred, String> {
    let cancel = session.cancel.clone();
    cancel.store(false, Ordering::Relaxed);
    blocking(move || {
        let clone = banker_core::clone::clone_folder(&home(), &folder)?;
        let mut progress = |p: &Progress| {
            let _ = app.emit("scan-progress", p);
        };
        banker_core::transfer::transfer(&clone, &device, &selection, &album, &cancel, &mut progress)
    })
    .await
}

#[tauri::command]
async fn get_location() -> Result<Location, String> {
    blocking(|| Ok(banker_core::settings::location(&home()))).await
}

/// Choose where scan copies, clones and saved items go; None for the default.
#[tauri::command]
async fn set_location(folder: Option<String>) -> Result<Location, String> {
    blocking(move || banker_core::settings::set_save_to(&home(), folder.as_deref())).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanCopy {
    path: String,
    bytes: u64,
}

/// The scan copy of a phone, if one is kept.
#[tauri::command]
async fn scan_copy(device: Device) -> Result<Option<ScanCopy>, String> {
    blocking(move || {
        let Ok(root) = banker_core::settings::save_root(&home()) else {
            return Ok(None);
        };
        let dir = banker_core::scan::scan_copy_dir(&root, &device);
        Ok(dir.is_dir().then(|| ScanCopy {
            bytes: banker_core::settings::folder_size(&dir),
            path: dir.display().to_string(),
        }))
    })
    .await
}

#[tauri::command]
async fn delete_scan_copy(device: Device) -> Result<(), String> {
    blocking(move || {
        let root = banker_core::settings::save_root(&home())?;
        let dir = banker_core::scan::scan_copy_dir(&root, &device);
        std::fs::remove_dir_all(&dir)
            .map_err(|e| format!("Could not delete {}: {e}", dir.display()))
    })
    .await
}

#[tauri::command]
fn cancel_scan(session: State<'_, Session>) {
    session.cancel.store(true, Ordering::Relaxed);
}

#[tauri::command]
fn query(session: State<'_, Session>, query: Query) -> Result<QueryResult, String> {
    let report = session.report.lock().map_err(|e| e.to_string())?;
    let findings = report
        .as_ref()
        .map(|r| r.findings.as_slice())
        .unwrap_or(&[]);
    Ok(banker_core::timeline::query(findings, &query))
}

/// The finding with this id in the current report.
fn finding(session: &Session, id: u64) -> Result<Option<banker_core::Finding>, String> {
    let report = session.report.lock().map_err(|e| e.to_string())?;
    Ok(report.as_ref().and_then(|r| r.finding(id)).cloned())
}

#[tauri::command]
async fn card_preview(
    app: AppHandle,
    session: State<'_, Session>,
    id: u64,
) -> Result<CardPreview, String> {
    let cache = work_dir(&app)?.join("thumbnails");
    let finding = finding(&session, id)?;
    blocking(move || {
        Ok(finding
            .map(|f| banker_core::preview::card_preview(&f, &cache))
            .unwrap_or_default())
    })
    .await
}

/// The full preview. When it shows a file directly (a picture, a player, a
/// PDF), only that one file is opened to the window's asset protocol.
#[tauri::command]
async fn preview(app: AppHandle, session: State<'_, Session>, id: u64) -> Result<Preview, String> {
    let cache = work_dir(&app)?.join("thumbnails");
    let finding = finding(&session, id)?.ok_or("That item is no longer in the results.")?;
    let shown = blocking(move || Ok(banker_core::preview::preview(&finding, &cache))).await?;
    if let Some(file) = shown.file() {
        app.asset_protocol_scope()
            .allow_file(file)
            .map_err(|e| e.to_string())?;
    }
    Ok(shown)
}

#[tauri::command]
async fn export(session: State<'_, Session>, query: Query) -> Result<Exported, String> {
    let store = session.report.clone();
    blocking(move || {
        let report = store.lock().map_err(|e| e.to_string())?;
        let report = report.as_ref().ok_or("Scan a device first.")?;
        banker_core::export::export_matching(report, &query, &home(), now())
            .map_err(|e| format!("Could not save: {e}"))
    })
    .await
}

#[tauri::command]
fn reveal(path: String) -> Result<(), String> {
    banker_core::export::reveal(&path)
}

/// Build and run the application.
pub fn run() {
    tauri::Builder::default()
        .manage(Session::default())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // The phone tools bundled inside the app (scripts/bundle-tools.sh).
            // The core looks there before PATH and Homebrew.
            if let Ok(dir) = app.path().resource_dir() {
                std::env::set_var("BANKER_TOOLS_DIR", dir.join("tools").join("bin"));
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            discover,
            scan,
            cancel_scan,
            get_location,
            set_location,
            scan_copy,
            delete_scan_copy,
            clone_device,
            list_clones,
            restore_clone,
            clone_groups,
            clone_items,
            clone_thumbnail,
            transfer_clone,
            query,
            card_preview,
            preview,
            export,
            reveal
        ])
        .run(tauri::generate_context!())
        .expect("failed to start the application");
}
