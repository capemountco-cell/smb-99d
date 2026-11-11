/**
 * Every call from the UI to Rust goes through this file, so the command
 * surface is visible in one place and argument shapes cannot drift.
 * The types mirror the serde shapes in crates/banker-core.
 */

import { invoke } from "@tauri-apps/api/core";
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
  tool: string;
  why: string;
  install: string;
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
export const query = (q: Query) => call<QueryResult>("query", { query: q });
export const thumbnail = (id: number) => call<string | null>("thumbnail", { id });
export const exportMatching = (q: Query) => call<Exported>("export", { query: q });
export const reveal = (path: string) => call<void>("reveal", { path });

export function onProgress(handler: (p: Progress) => void): Promise<UnlistenFn> {
  if (!nativeAvailable()) return Promise.resolve(() => {});
  return listen<Progress>("scan-progress", (event) => handler(event.payload));
}
