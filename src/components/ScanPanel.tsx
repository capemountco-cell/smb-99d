import { useEffect, useState } from "react";

import { bytes, count } from "../lib/format";
import {
  deleteScanCopy,
  scanCopy,
  type CloneEntry,
  type Depth,
  type Device,
  type Location,
  type Progress,
  type ScanCopy,
  type ScanSummary,
  type Transferred,
} from "../lib/native";
import { SaveTo } from "./SaveTo";
import { Icon, type IconName } from "./Icon";

export type Stage =
  | { kind: "idle" }
  | { kind: "scanning"; device: Device; depth: Depth; progress: Progress | null }
  | { kind: "cloning"; device: Device; progress: Progress | null; zip: boolean }
  | { kind: "restoring"; device: Device; progress: Progress | null }
  | { kind: "cloned"; device: Device; clone: CloneEntry }
  | { kind: "restored"; device: Device; message: string }
  | { kind: "awaiting"; clone: CloneEntry }
  | { kind: "choosing"; device: Device; clone: CloneEntry }
  | { kind: "transferring"; device: Device; progress: Progress | null }
  | { kind: "transferred"; device: Device; result: Transferred }
  | { kind: "failed"; message: string };

export type Working = Extract<Stage, { kind: "scanning" | "cloning" | "restoring" | "transferring" }>;

interface Props {
  device: Device | null;
  stage: Stage;
  clones: CloneEntry[];
  /** The last scan of this device in this session, if any. */
  lastScan: ScanSummary | null;
  onScan: (device: Device, depth: Depth) => void;
  onClone: (device: Device) => void;
  onRestore: (device: Device) => void;
  onMove: (device: Device, clone: CloneEntry) => void;
  onOpenResults: () => void;
  onChooseDevice: () => void;
  onCancel: () => void;
  location: Location | null;
  onLocation: (location: Location) => void;
}

const CAPTION: Record<Device["kind"], string> = {
  mac: "Reco reads this Mac where it is and copies nothing while it looks",
  drive: "Reco reads the drive where it is and changes nothing on it",
  folder: "Reco reads the folder where it is and changes nothing in it",
  backup: "Reco reads this backup and changes nothing",
  iphone: "Nothing on the phone changes",
  android: "Nothing on the phone changes",
};

type Feature = { icon: IconName; label: string };
const PHONE_FEATURES: Feature[] = [
  { icon: "photo", label: "Deleted photos and videos" },
  { icon: "message", label: "Deleted messages, app by app" },
  { icon: "damaged", label: "Damaged files that won't open" },
];
const DRIVE_FEATURES: Feature[] = [
  { icon: "trash", label: "Files in its trash folders" },
  { icon: "database", label: "Deleted records in app data" },
  { icon: "damaged", label: "Damaged files that won't open" },
];
/** What a scan finds, worded for the kind of device. */
const FEATURES: Record<Device["kind"], Feature[]> = {
  mac: DRIVE_FEATURES,
  iphone: PHONE_FEATURES,
  android: PHONE_FEATURES,
  backup: PHONE_FEATURES,
  drive: DRIVE_FEATURES,
  folder: DRIVE_FEATURES,
};

/** Long names step down in size so they never run edge to edge. */
const titleSize = (name: string) => (name.length > 32 ? "xl long" : name.length > 20 ? "xl longish" : "xl");

type Step = { label: string; state: "done" | "now" | "next" };

/** The visible steps of each kind of work, worked out from its phase. */
function steps(stage: Working): Step[] {
  const phase = stage.progress?.phase ?? "";
  const mark = (labels: string[], at: number): Step[] =>
    labels.map((label, i) => ({ label, state: i < at ? "done" : i === at ? "now" : "next" }));

  if (stage.kind === "cloning") {
    const labels = ["Connect", "Copy everything", "Organise", ...(stage.zip ? ["Make the .zip"] : [])];
    const at = /zip/i.test(phase) ? 3 : /organis/i.test(phase) ? 2 : /clon|copy/i.test(phase) ? 1 : 0;
    return mark(labels, at);
  }
  if (stage.kind === "transferring") {
    const at = /gallery/i.test(phase) ? 2 : /send/i.test(phase) ? 1 : 0;
    return mark(["Prepare", "Send to the phone", "Add to the Gallery"], at);
  }
  if (stage.kind === "restoring") {
    return mark(["Prepare the clone", "Restore"], /restor|copying the files/i.test(phase) ? 1 : 0);
  }
  const copies = stage.device.kind === "iphone" || stage.device.kind === "android";
  if (copies && stage.depth === "photos") {
    const at = /fetching/i.test(phase) ? 1 : /asking|reading the iphone/i.test(phase) ? 0 : 2;
    return mark(["Find the deleted photos", "Fetch only those", "Get them ready to look at"], at);
  }
  const copying = copies && /backing up|copying/i.test(phase);
  return copies
    ? mark(["Copy to the Save to location", "Look for deleted and damaged items"], copying ? 0 : 1)
    : mark(["Look for deleted and damaged items"], 0);
}

const VERB: Record<Working["kind"], string> = {
  scanning: "Reading",
  cloning: "Cloning",
  restoring: "Restoring",
  transferring: "Transferring to",
};

/** Reco is working: the ring wears the section's colour, and fills when the percentage is known. */
export function WorkingView({ stage, onCancel }: { stage: Working; onCancel: () => void }) {
  const p = stage.progress;
  const phase = p?.phase ?? "Starting…";
  const percent = p?.percent ?? null;
  return (
    <div className="hero working-view">
      <span className="state-pill working">
        <span className="dot working" />
        Working
      </span>
      <h1 className="display">
        {VERB[stage.kind]} {stage.device.name}
      </h1>
      <p className="lede">{phase}. You can keep using your Mac.</p>
      {percent !== null ? (
        <div
          className="ring known"
          role="progressbar"
          aria-label={phase}
          aria-valuenow={percent}
          aria-valuemin={0}
          aria-valuemax={100}
          style={{ ["--done" as string]: `${percent}%` }}
        >
          <div className="ring-inner">
            <span className="ring-number">{percent}%</span>
          </div>
        </div>
      ) : (
        <div className="ring" role="progressbar" aria-label={phase}>
          <div className="ring-inner">
            {stage.kind === "scanning" && p && p.files > 0 ? (
              <>
                <span className="ring-number">{p.files.toLocaleString()}</span>
                <span className="muted small-text">files read</span>
              </>
            ) : (
              <span className="muted small-text">working</span>
            )}
          </div>
        </div>
      )}
      <ol className="steps">
        {steps(stage).map((s) => (
          <li key={s.label} className={`step ${s.state}`}>
            {s.state === "done" ? <Icon name="check" size={16} stroke={2.2} /> : <span className="step-mark" />}
            {s.label}
          </li>
        ))}
      </ol>
      <div className="row center-row">
        {stage.kind === "scanning" && p && p.findings > 0 && (
          <span className="lede">
            <strong>{count(p.findings, "item")}</strong> found so far
          </span>
        )}
        <button type="button" className="ghost" onClick={onCancel}>
          Stop
        </button>
      </div>
      {p?.current && <p className="muted path">{p.current}</p>}
    </div>
  );
}

/** An action card: always clickable. When it cannot run yet, it says why. */
interface Action {
  icon: IconName;
  title: string;
  /** What it does, or why it cannot run yet and what to do. */
  line: string;
  ready: boolean;
  run: () => void;
}

const KIND_NAME: Record<Device["kind"], string> = {
  mac: "Mac",
  iphone: "iPhone",
  android: "Android phone",
  drive: "drive",
  folder: "folder",
  backup: "backup",
};

export function ScanPanel(props: Props) {
  const { device, stage, clones, lastScan, onScan, onCancel } = props;
  const [notice, setNotice] = useState<string | null>(null);
  const [copy, setCopy] = useState<ScanCopy | null>(null);
  const phoneId = device && (device.kind === "iphone" || device.kind === "android") ? device.id : null;

  // The phone's scan copy, if one is kept: shown with a way to delete it.
  useEffect(() => {
    setCopy(null);
    if (!device || !phoneId) return;
    let live = true;
    scanCopy(device)
      .then((c) => live && setCopy(c))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [phoneId, lastScan, props.location?.path]);

  if (
    stage.kind === "scanning" ||
    stage.kind === "cloning" ||
    stage.kind === "restoring" ||
    stage.kind === "transferring"
  ) {
    return <WorkingView stage={stage} onCancel={onCancel} />;
  }

  if (!device) {
    return (
      <div className="home empty-home">
        <span className="status-chip">
          <span className="chip-dot hollow" />
          No device chosen
        </span>
        <h1 className="home-title">No phone connected</h1>
        <p className="home-lede">
          Plug in an iPhone or an Android phone with a cable. It shows up here by itself.
        </p>
        <button type="button" className="secondary big" onClick={props.onChooseDevice}>
          Show every device
        </button>
      </div>
    );
  }

  const phone = device.kind === "iphone" || device.kind === "android";
  const needs = device.problem;
  const own = clones.filter((c) => c.info.deviceId === device.id);
  const fromOther = clones.find(
    (c) =>
      c.info.deviceId !== device.id &&
      (device.kind === "android" || (c.info.kind === "iphone" && (c.backupPresent || c.zipPresent))),
  );
  const sameKind = clones.filter(
    (c) => c.info.kind === device.kind && (c.backupPresent || c.zipPresent),
  );
  const blocked = needs ? `First: ${needs}` : null;

  const actions: Action[] = phone
    ? [
        {
          icon: "copy",
          title: "Clone this phone",
          line: blocked ?? "A full copy, on your Save to location, that can restore it or set up a new phone.",
          ready: !needs,
          run: () => props.onClone(device),
        },
        {
          icon: "transfer",
          title: "Move files from another phone",
          line:
            blocked ??
            (fromOther
              ? `From the clone of ${fromOther.info.deviceName}. Pick files or move everything.`
              : "No clone of another phone on this Mac yet. Clone the old phone first, then plug this one in."),
          ready: !needs && fromOther !== undefined,
          run: () => fromOther && props.onMove(device, fromOther),
        },
        {
          icon: "refresh",
          title: "Restore from a clone",
          line:
            blocked ??
            (sameKind.length > 0
              ? device.kind === "iphone"
                ? "Erases this iPhone and puts a clone back on it. Asks before anything happens."
                : "Copies a clone's photos and files back onto this phone."
              : `No clone of an ${KIND_NAME[device.kind]} on this Mac yet. Clone one first.`),
          ready: !needs && sameKind.length > 0,
          run: () => props.onRestore(device),
        },
      ]
    : [];

  const status = needs ? "Needs you" : device.kind === "drive" ? "Connected · read in place" : "Ready";

  return (
    <div className="home">
      <section className="home-main">
        <span className={`status-chip ${needs ? "needs" : ""}`}>
          <span className={`chip-dot ${needs ? "hollow" : ""}`} />
          {status}
        </span>
        <h1 className={`home-title ${titleSize(device.name)}`}>{device.name}</h1>
        <p className="home-lede">
          {CAPTION[device.kind]}. It lists only what was deleted or damaged.
        </p>
        {needs && <p className="needs-box">{needs}</p>}
        {stage.kind === "failed" && <p className="banner error selectable">{stage.message}</p>}
        {phone ? (
          <div className="depth-cards">
            <section className="depth-card recommended">
              <div className="depth-head">
                <h2>Quick photo scan</h2>
                <span className="muted small-text">recommended</span>
              </div>
              <p className="muted">
                Reads the deleted photos and videos straight off the phone. Nothing else is copied.
              </p>
              <ul className="depth-points">
                <li><span className="status-mark ok" />Recently Deleted photos and videos, with previews</li>
                <li><span className="status-mark ok" />Pick and recover them one at a time</li>
              </ul>
              <div className="depth-foot">
                <button type="button" className="primary" disabled={!!needs} onClick={() => onScan(device, "photos")}>
                  Scan photos
                </button>
                <span className="small-text">Space needed <strong>almost none</strong></span>
              </div>
            </section>
            <section className="depth-card">
              <div className="depth-head">
                <h2>Deep scan</h2>
                <span className="muted small-text">messages, notes, app data</span>
              </div>
              <p className="muted">
                {device.kind === "iphone"
                  ? "Apple only hands these over inside a backup, so the phone is copied once. The copy is reused next time."
                  : "The whole phone is copied once, so app files can be read too. The copy is reused next time."}
              </p>
              <ul className="depth-points">
                <li><span className="status-mark ok" />Deleted messages, notes and app records</li>
                <li>
                  <span className={`status-mark ${props.location?.available === false ? "bad" : "warn"}`} />
                  Needs room for the whole phone on {props.location?.label ?? "this Mac"}
                  {props.location?.freeBytes != null && ` (${bytes(props.location.freeBytes)} free)`}. Checked before it starts.
                </li>
              </ul>
              <div className="depth-foot">
                <button type="button" className="secondary" disabled={!!needs} onClick={() => onScan(device, "everything")}>
                  Copy and scan
                </button>
                <SaveTo location={props.location} onChange={props.onLocation} compact />
              </div>
            </section>
          </div>
        ) : (
          <div className="home-cta">
            <button type="button" className="primary big" disabled={!!needs} onClick={() => onScan(device, "everything")}>
              <Icon name="search" size={18} stroke={2.2} />
              Scan for deleted items
            </button>
          </div>
        )}
        {lastScan && (
          <button type="button" className="text-button" onClick={props.onOpenResults}>
            See the last results ›
          </button>
        )}
        <div className="stat-tiles">
          <div className="stat">
            <strong>{lastScan ? lastScan.findings.toLocaleString() : "—"}</strong>
            <span>{lastScan ? "found in the last scan" : "Not scanned yet"}</span>
          </div>
          <div className="stat">
            <strong>{phone ? own.length || "None" : KIND_NAME[device.kind]}</strong>
            <span>{phone ? `${own.length === 1 ? "Clone" : "Clones"} of this phone` : "Kind"}</span>
          </div>
          {phone ? (
            <div className="stat">
              <strong>{copy ? bytes(copy.bytes) : "None"}</strong>
              <span>Scan copy on {props.location?.label ?? "this Mac"}</span>
              {copy && (
                <button
                  type="button"
                  className="stat-action"
                  onClick={() =>
                    void deleteScanCopy(device)
                      .then(() => setCopy(null))
                      .catch((e) => setNotice(String(e)))
                  }
                >
                  Delete
                </button>
              )}
            </div>
          ) : (
            <div className="stat">
              <strong>{lastScan ? lastScan.filesChecked.toLocaleString() : "—"}</strong>
              <span>Files checked</span>
            </div>
          )}
        </div>
      </section>

      <section className="home-side" aria-label={phone ? "More for this phone" : "What a scan finds"}>
        <span className="section-label">{phone ? "More for this phone" : "What a scan finds"}</span>
        {phone ? (
          <>
            {actions.map((a) => (
              <button
                key={a.title}
                type="button"
                className={`action-card ${a.ready ? "" : "waiting"}`}
                onClick={() => {
                  if (a.ready) {
                    setNotice(null);
                    a.run();
                  } else {
                    setNotice(`${a.title}: ${a.line}`);
                  }
                }}
              >
                <span className="action-tile">
                  <Icon name={a.icon} size={20} />
                </span>
                <span className="action-text">
                  <span className="action-title">{a.title}</span>
                  <span className="action-line">{a.line}</span>
                </span>
                <Icon name="next" size={16} stroke={2.2} />
              </button>
            ))}
            {notice && (
              <p className="notice" role="status">
                {notice}
              </p>
            )}
          </>
        ) : (
          FEATURES[device.kind].map((f) => (
            <div key={f.label} className="action-card static">
              <span className="action-tile">
                <Icon name={f.icon} size={20} />
              </span>
              <span className="action-text">
                <span className="action-title">{f.label}</span>
              </span>
            </div>
          ))
        )}
      </section>
    </div>
  );
}

/** Results with nothing scanned yet: say how to start, never a dead end. */
export function NoResults({ device, onScan, onDevice }: { device: Device | null; onScan: () => void; onDevice: () => void }) {
  return (
    <div className="home empty-home">
      <span className="status-chip">
        <span className="chip-dot hollow" />
        No results yet
      </span>
      <h1 className="home-title">Nothing scanned yet</h1>
      <p className="home-lede">
        {device
          ? `Scan ${device.name} and everything deleted or damaged on it shows up here, on a timeline.`
          : "Choose a device and scan it. Everything deleted or damaged shows up here, on a timeline."}
      </p>
      {device && !device.problem ? (
        <button type="button" className="primary big" onClick={onScan}>
          <Icon name="search" size={18} stroke={2.2} />
          Scan {device.name}
        </button>
      ) : (
        <button type="button" className="secondary big" onClick={onDevice}>
          Go to the device
        </button>
      )}
    </div>
  );
}
