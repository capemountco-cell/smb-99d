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
use banker_core::find::{FindSettings, Found};
use banker_core::preview::{CardPreview, Preview};
use banker_core::scan::{Depth, Progress};
use banker_core::settings::Location;
use banker_core::timeline::{Query, QueryResult};
use banker_core::transfer::{Group, Item, Selection, Transferred};
use banker_core::Report;
use serde::Serialize;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

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
    depth: Option<Depth>,
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
            depth.unwrap_or_default(),
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

/// Copy the picked items (one photo, or many) to the Save to location.
#[tauri::command]
async fn recover(
    session: State<'_, Session>,
    ids: Vec<u64>,
    tz_offset_minutes: i32,
) -> Result<Exported, String> {
    let store = session.report.clone();
    blocking(move || {
        let report = store.lock().map_err(|e| e.to_string())?;
        let report = report.as_ref().ok_or("Scan a device first.")?;
        banker_core::export::recover(report, &ids, &home(), now(), tz_offset_minutes)
    })
    .await
}

#[tauri::command]
fn full_disk_access() -> bool {
    banker_core::export::has_full_disk_access(&home())
}

#[tauri::command]
fn open_full_disk_access() -> Result<(), String> {
    banker_core::export::open_full_disk_access()
}

#[tauri::command]
fn reveal(path: String) -> Result<(), String> {
    banker_core::export::reveal(&path)
}

// ---- Find -------------------------------------------------------------

/// Where apps are installed, to see which launchers might hold ⌥Space.
fn apps_dir() -> PathBuf {
    PathBuf::from("/Applications")
}

/// Show the Find panel over whatever app is in front, ready to type.
fn show_find(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("find") {
        let _ = w.center();
        let _ = w.show();
        let _ = w.set_focus();
        let _ = w.emit("find-open", ());
    }
}

fn toggle_find(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("find") {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
        } else {
            show_find(app);
        }
    }
}

fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Register the shortcut that opens Find. An app that already holds it
/// makes macOS refuse, which is reported in words.
fn register_shortcut(app: &AppHandle, shortcut: &str) -> Result<(), String> {
    let parsed: Shortcut = banker_core::find::normalize(shortcut)
        .parse()
        .map_err(|e| format!("Reco does not understand that shortcut: {e}"))?;
    app.global_shortcut().register(parsed).map_err(|_| {
        "Another app is already using that shortcut. Choose another, such as ⌃⌥Space.".to_string()
    })
}

#[tauri::command]
async fn find_files(
    session: State<'_, Session>,
    query: String,
    drop: Vec<String>,
    tz_offset_minutes: i32,
) -> Result<Found, String> {
    let store = session.report.clone();
    blocking(move || {
        let report = store.lock().map_err(|e| e.to_string())?.clone();
        Ok(banker_core::find::find(
            &query,
            &drop,
            report.as_ref(),
            &home(),
            now(),
            tz_offset_minutes,
        ))
    })
    .await
}

#[tauri::command]
async fn find_thumbnail(app: AppHandle, path: String) -> Result<Option<String>, String> {
    let cache = work_dir(&app)?.join("thumbnails");
    blocking(move || Ok(banker_core::find::thumbnail(&path, &cache))).await
}

#[tauri::command]
fn open_path(path: String) -> Result<(), String> {
    banker_core::export::open(&path)
}

#[tauri::command]
async fn recover_paths(paths: Vec<String>, tz_offset_minutes: i32) -> Result<String, String> {
    blocking(move || banker_core::find::recover_paths(&paths, &home(), now(), tz_offset_minutes))
        .await
}

#[tauri::command]
fn hide_find(app: AppHandle) {
    if let Some(w) = app.get_webview_window("find") {
        let _ = w.hide();
    }
}

#[tauri::command]
fn open_main(app: AppHandle) {
    hide_find(app.clone());
    show_main_window(&app);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FindPrefs {
    #[serde(flatten)]
    find: FindSettings,
    starts_at_login: bool,
}

fn prefs(app: &AppHandle) -> FindPrefs {
    FindPrefs {
        find: banker_core::find::find_settings(&home(), &apps_dir()),
        starts_at_login: app.autolaunch().is_enabled().unwrap_or(false),
    }
}

#[tauri::command]
fn get_find_settings(app: AppHandle) -> FindPrefs {
    prefs(&app)
}

/// Change the shortcut: refuse macOS's own, register the new one, and only
/// then let go of the old one, so Find is never left without a shortcut.
#[tauri::command]
fn set_find_shortcut(app: AppHandle, shortcut: String) -> Result<FindPrefs, String> {
    banker_core::find::check_shortcut(&shortcut, &apps_dir())?;
    let new = banker_core::find::normalize(&shortcut);
    let old = prefs(&app).find.shortcut;
    if banker_core::find::normalize(&old) == new {
        return Ok(prefs(&app));
    }
    register_shortcut(&app, &new)?;
    if let Ok(old) = banker_core::find::normalize(&old).parse::<Shortcut>() {
        let _ = app.global_shortcut().unregister(old);
    }
    banker_core::find::update_find_settings(&home(), |s| s.find_shortcut = Some(new))?;
    Ok(prefs(&app))
}

#[tauri::command]
fn set_find_deleted(app: AppHandle, on: bool) -> Result<FindPrefs, String> {
    banker_core::find::update_find_settings(&home(), |s| s.find_deleted = Some(on))?;
    Ok(prefs(&app))
}

#[tauri::command]
fn set_find_excludes(app: AppHandle, excludes: Vec<String>) -> Result<FindPrefs, String> {
    banker_core::find::update_find_settings(&home(), |s| s.find_exclude = Some(excludes))?;
    Ok(prefs(&app))
}

#[tauri::command]
fn set_starts_at_login(app: AppHandle, on: bool) -> Result<FindPrefs, String> {
    let launcher = app.autolaunch();
    if on {
        launcher.enable()
    } else {
        launcher.disable()
    }
    .map_err(|e| e.to_string())?;
    Ok(prefs(&app))
}

/// Reco in the menu bar: Find, the window, and Quit (closing the window
/// keeps Reco running so the shortcut keeps working).
fn tray(app: &tauri::App) -> tauri::Result<()> {
    let find = MenuItem::with_id(app, "find", "Find…", true, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open Reco", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Reco", true, None::<&str>)?;
    let line = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&find, &open, &line, &quit])?;
    let mut builder = TrayIconBuilder::with_id("reco")
        .tooltip("Reco")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "find" => show_find(app),
            "open" => show_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

/// Build and run the application.
pub fn run() {
    tauri::Builder::default()
        .manage(Session::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        toggle_find(app);
                    }
                })
                .build(),
        )
        .setup(|app| {
            // The phone tools bundled inside the app (scripts/bundle-tools.sh).
            // The core looks there before PATH and Homebrew.
            if let Ok(dir) = app.path().resource_dir() {
                std::env::set_var("BANKER_TOOLS_DIR", dir.join("tools").join("bin"));
            }
            let shortcut = banker_core::find::find_settings(&home(), &apps_dir()).shortcut;
            if let Err(e) = register_shortcut(app.handle(), &shortcut) {
                eprintln!("Find shortcut {shortcut}: {e}");
            }
            if let Err(e) = tray(app) {
                eprintln!("menu bar icon: {e}");
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // Closing the window keeps Reco running, so ⌥Space still works.
            WindowEvent::CloseRequested { api, .. } if window.label() == "main" => {
                api.prevent_close();
                let _ = window.hide();
            }
            // The Find panel goes away when you click elsewhere.
            WindowEvent::Focused(false) if window.label() == "find" => {
                let _ = window.hide();
            }
            _ => {}
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
            recover,
            find_files,
            find_thumbnail,
            open_path,
            recover_paths,
            hide_find,
            open_main,
            get_find_settings,
            set_find_shortcut,
            set_find_deleted,
            set_find_excludes,
            set_starts_at_login,
            full_disk_access,
            open_full_disk_access,
            reveal
        ])
        .build(tauri::generate_context!())
        .expect("failed to start the application")
        .run(|app, event| {
            // Clicking Reco in the Dock brings the window back.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                show_main_window(app);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        });
}
