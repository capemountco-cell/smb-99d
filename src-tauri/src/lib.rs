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

use banker_core::devices::{Device, Discovery};
use banker_core::export::Exported;
use banker_core::scan::Progress;
use banker_core::timeline::{Query, QueryResult};
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

#[tauri::command]
async fn thumbnail(
    app: AppHandle,
    session: State<'_, Session>,
    id: u64,
) -> Result<Option<String>, String> {
    let cache = work_dir(&app)?.join("thumbnails");
    let finding = {
        let report = session.report.lock().map_err(|e| e.to_string())?;
        report.as_ref().and_then(|r| r.finding(id)).cloned()
    };
    blocking(move || Ok(finding.and_then(|f| banker_core::export::thumbnail(&f, &cache)))).await
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
        .invoke_handler(tauri::generate_handler![
            discover,
            scan,
            cancel_scan,
            query,
            thumbnail,
            export,
            reveal
        ])
        .run(tauri::generate_context!())
        .expect("failed to start the application");
}
