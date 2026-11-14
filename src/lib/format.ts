/**
 * Display helpers. Pure functions only: what to show, never what to find.
 */

import type { IconName } from "../components/Icon";
import type { DeviceKind, Media, Source, Zoom } from "./native";

export const ZOOMS: Zoom[] = ["year", "month", "day", "hour", "minute"];

/** The next finer zoom, for drilling into a bucket. */
export function finer(zoom: Zoom): Zoom {
  const i = ZOOMS.indexOf(zoom);
  return ZOOMS[i + 1] ?? "minute";
}

export function bytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = n / 1024;
  let u = 0;
  while (v >= 1024 && u < units.length - 1) {
    v /= 1024;
    u += 1;
  }
  return `${v < 10 ? v.toFixed(1) : Math.round(v)} ${units[u]}`;
}

export function count(n: number, one: string, many = `${one}s`): string {
  return `${n.toLocaleString()} ${n === 1 ? one : many}`;
}

export function when(unix: number | null, estimated: boolean): string {
  if (unix === null) return "Date unknown";
  const text = new Date(unix * 1000).toLocaleString(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  });
  return estimated ? `≈ ${text}` : text;
}

export const DEVICE_ICON: Record<DeviceKind, IconName> = {
  mac: "mac",
  drive: "drive",
  iphone: "iphone",
  android: "android",
  backup: "folder",
  folder: "folder",
};

export const MEDIA_LABEL: Record<Media, string> = {
  image: "Photos",
  video: "Videos",
  audio: "Audio",
  document: "Documents",
  archive: "Archives",
  text: "Messages & records",
  database: "Databases",
  other: "Other",
};

export const MEDIA_ICON: Record<Media, IconName> = {
  image: "photo",
  video: "video",
  audio: "audio",
  document: "document",
  archive: "archive",
  text: "message",
  database: "database",
  other: "file",
};

export const SOURCE_LABEL: Record<Source, string> = {
  trashFolder: "Trash",
  photosRecentlyDeleted: "Recently Deleted",
  databaseFragment: "Deleted record",
  databaseHistory: "Earlier version",
  carvedFromDatabase: "Rebuilt picture",
  damagedFile: "Damaged",
};

/** Local calendar parts of a Unix time, in this machine's time zone. */
export function localParts(unix: number) {
  const d = new Date(unix * 1000);
  return { year: d.getFullYear(), month: d.getMonth(), day: d.getDate(), weekday: d.getDay() };
}

export function monthTitle(year: number, month: number): string {
  return new Date(year, month, 1).toLocaleDateString(undefined, {
    month: "long",
    year: "numeric",
  });
}

export function daysInMonth(year: number, month: number): number {
  return new Date(year, month + 1, 0).getDate();
}
