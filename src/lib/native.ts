/**
 * Every call from the UI to Rust goes through this file, so the command
 * surface is visible in one place and argument shapes cannot drift.
 * The types mirror the serde shapes in crates/banker-core.
 */

import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Status = "deleted" | "corrupt";
export type Source =
  | "trashFolder"
  | "photosRecentlyDeleted"
  | "databaseFragment"
  | "databaseHistory"
  | "carvedFromDatabase"
  | "damagedFile";
export type Media =
  | "image"
  | "video"
  | "audio"
  | "document"
  | "archive"
  | "text"
  | "database"
  | "other";
export type Zoom = "year" | "month" | "day" | "hour" | "minute";
export type DeviceKind = "drive" | "iphone" | "android" | "backup" | "folder";

export interface Finding {
  id: number;
  status: Status;
  source: Source;
  media: Media;
  name: string;
  reason: string;
  path: string | null;
  originalPath: string | null;
  app: string | null;
  size: number;
  time: number | null;
  timeEstimated: boolean;
  text: string | null;
}

export interface Device {
  id: string;
  kind: DeviceKind;
  name: string;
  detail: string;
  path: string | null;
  problem: string | null;
}

export interface ToolHint {
  text: string;
  command: string | null;
}

export interface Discovery {
  devices: Device[];
  hints: ToolHint[];
}

export interface Progress {
  phase: string;
  files: number;
  findings: number;
  current: string;
  /** How far along, when known (iPhone backups and restores). */
  percent: number | null;
}

export interface ScanSummary {
  root: string;
  filesChecked: number;
  databasesChecked: number;
  findings: number;
  notes: string[];
}

export interface Query {
  text: string;
  app: string | null;
  status: Status | null;
  media: Media | null;
  zoom: Zoom;
  from: number | null;
  to: number | null;
  tzOffsetMinutes: number;
  limit: number;
}

export interface Bucket {
  start: number;
  end: number;
  label: string;
  count: number;
  /** How many of `count` are damaged rather than deleted. */
  corrupt: number;
}

export interface AppCount {
  id: string;
  name: string;
  count: number;
}

export interface QueryResult {
  buckets: Bucket[];
  items: Finding[];
  total: number;
  undated: number;
  apps: AppCount[];
  deleted: number;
  corrupt: number;
  byMedia: Partial<Record<Media, number>>;
  bySource: Partial<Record<Source, number>>;
}

export interface CardPreview {
  /** A small picture, as a data URL. */
  image: string | null;
  /** The opening lines, for text. */
  text: string | null;
}

export type Preview =
  | { kind: "image"; path: string }
  | { kind: "video"; path: string }
  | { kind: "audio"; path: string }
  | { kind: "pdf"; path: string }
  | { kind: "text"; text: string; truncated: boolean }
  | { kind: "picture"; dataUrl: string }
  | { kind: "nothing"; reason: string };

export interface CloneOptions {
  /** iPhone only: a backup password, so passwords, Health and Wi-Fi are included. */
  password: string | null;
  makeZip: boolean;
}

export interface CloneInfo {
  version: number;
  deviceName: string;
  kind: DeviceKind;
  deviceId: string;
  model: string | null;
  osVersion: string | null;
  created: number;
  encrypted: boolean;
  zip: string | null;
  photos: number;
  appFiles: number;
  otherFiles: number;
  bytes: number;
  notes: string[];
}

export interface CloneEntry {
  folder: string;
  info: CloneInfo;
  backupPresent: boolean;
  zipPresent: boolean;
  scanPath: string | null;
}

export interface CloneGroup {
  /** Path inside the clone, e.g. "Photos & Videos" or "Apps/WhatsApp". */
  id: string;
  label: string;
  media: Media;
  files: number;
  bytes: number;
  sample: string[];
}

export interface CloneItem {
  path: string;
  name: string;
  media: Media;
  size: number;
  time: number | null;
}

export interface Selection {
  groups: string[];
  files: string[];
}

export interface Transferred {
  files: number;
  bytes: number;
  notes: string[];
}

export interface Exported {
  folder: string;
  files: number;
  records: number;
  failed: number;
}

/** True inside the Tauri window; false in a plain browser tab. */
export function nativeAvailable(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!nativeAvailable()) {
    throw new Error("The native side is not running. Start the app with `npm run dev`.");
  }
  return invoke<T>(command, args);
}

/** Minutes east of UTC, which is what the core expects. */
export const localOffsetMinutes = () => -new Date().getTimezoneOffset();

export const discover = () => call<Discovery>("discover");
export const scan = (device: Device) =>
  call<ScanSummary>("scan", { device, tzOffsetMinutes: localOffsetMinutes() });
export const cancelScan = () => call<void>("cancel_scan");
export const cloneDevice = (device: Device, options: CloneOptions) =>
  call<CloneEntry>("clone_device", { device, options, tzOffsetMinutes: localOffsetMinutes() });
export const listClones = () => call<CloneEntry[]>("list_clones");
export const cloneGroups = (folder: string) => call<CloneGroup[]>("clone_groups", { folder });
export const cloneItems = (folder: string, group: string, offset: number, limit: number) =>
  call<[CloneItem[], number]>("clone_items", { folder, group, offset, limit });
export const cloneThumbnail = (folder: string, path: string) =>
  call<string | null>("clone_thumbnail", { folder, path });
export const transferClone = (folder: string, device: Device, selection: Selection, album: string) =>
  call<Transferred>("transfer_clone", { folder, device, selection, album });
export const restoreClone = (folder: string, device: Device, password: string | null) =>
  call<string>("restore_clone", { folder, device, password });
export const query = (q: Query) => call<QueryResult>("query", { query: q });
export const cardPreview = (id: number) => call<CardPreview>("card_preview", { id });
export const preview = (id: number) => call<Preview>("preview", { id });

/** A URL the window can load a previewed file from. */
export const fileUrl = (path: string) => convertFileSrc(path);
export const exportMatching = (q: Query) => call<Exported>("export", { query: q });
export const reveal = (path: string) => call<void>("reveal", { path });

export function onProgress(handler: (p: Progress) => void): Promise<UnlistenFn> {
  if (!nativeAvailable()) return Promise.resolve(() => {});
  return listen<Progress>("scan-progress", (event) => handler(event.payload));
}
