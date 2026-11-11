//! Banker's logic: find what was deleted or damaged on a device.
//!
//! Everything that makes a decision lives here, not in the Tauri crate and not
//! in React. This crate does not depend on Tauri, so it builds and its tests
//! run on Linux in CI, without a window, a webview or a Mac.
//!
//! Banker only ever surfaces deleted or damaged content. Healthy, live files
//! and rows are read only to tell them apart, and are never reported.

pub mod carve;
pub mod clone;
pub mod devices;
pub mod export;
pub mod integrity;
pub mod ios;
pub mod model;
pub mod paths;
pub mod preview;
pub mod scan;
pub mod sqlite;
pub mod timeline;
mod util;

pub use model::{Finding, Media, Report, Source, Status};
