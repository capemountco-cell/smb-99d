import { DEVICE_ICON, count } from "../lib/format";
import type { CloneEntry, Device, Progress, Transferred } from "../lib/native";
import { Icon, type IconName } from "./Icon";

export type Stage =
  | { kind: "idle" }
  | { kind: "scanning"; device: Device; progress: Progress | null }
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
  onScan: (device: Device) => void;
  onClone: (device: Device) => void;
  onRestore: (device: Device) => void;
  onCancel: () => void;
}

const CAPTION: Record<Device["kind"], string> = {
  drive: "Reads the drive in place · changes nothing on it",
  folder: "Reads the folder in place · changes nothing in it",
  backup: "Reads this backup · changes nothing",
  iphone: "Backs up to this Mac first · changes nothing on the phone",
  android: "Copies to this Mac first · changes nothing on the phone",
};

const FEATURES: { icon: IconName; label: string }[] = [
  { icon: "photo", label: "Deleted photos and videos" },
  { icon: "message", label: "Deleted messages, app by app" },
  { icon: "damaged", label: "Damaged files that won't open" },
];

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
  const copying = copies && /back|copy/i.test(phase);
  return copies
    ? mark(["Copy to this Mac", "Look for deleted and damaged items"], copying ? 0 : 1)
    : mark(["Look for deleted and damaged items"], 0);
}

const VERB: Record<Working["kind"], string> = {
  scanning: "Reading",
  cloning: "Cloning",
  restoring: "Restoring",
  transferring: "Transferring to",
};

/** Amber: Banker is working. A percentage fills the ring when it is known. */
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

export function ScanPanel({ device, stage, onScan, onClone, onRestore, onCancel }: Props) {
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
      <div className="hero">
        <div className="device-art empty" aria-hidden>
          <Icon name="search" size={56} stroke={1.4} />
        </div>
        <h1 className="display">Find what was deleted</h1>
        <p className="lede narrow">
          Connect a drive, an iPhone or an Android phone with a cable and pick it on the left.
          One press shows every deleted photo, message and file, and anything damaged.
        </p>
      </div>
    );
  }

  const phone = device.kind === "iphone" || device.kind === "android";
  return (
    <div className="hero">
      <div className="hero-row">
        <div className="device-art" aria-hidden>
          <Icon name={DEVICE_ICON[device.kind]} size={64} stroke={1.2} />
        </div>
        <div className="hero-text">
          <h1 className="display">{device.name}</h1>
          <p className="lede">
            See everything that was deleted from it, and anything damaged, on a timeline down to
            the minute.
          </p>
          <ul className="features">
            {FEATURES.map((f) => (
              <li key={f.label}>
                <span className="tile">
                  <Icon name={f.icon} />
                </span>
                {f.label}
              </li>
            ))}
          </ul>
        </div>
      </div>
      {device.problem && <p className="waiting-text narrow">{device.problem}</p>}
      {stage.kind === "failed" && <p className="banner error selectable">{stage.message}</p>}
      <div className="scan-wrap">
        <button
          type="button"
          className="scan-button"
          disabled={device.problem !== null}
          onClick={() => onScan(device)}
        >
          Scan
        </button>
      </div>
      <p className="muted small-text">{CAPTION[device.kind]}</p>
      {phone && (
        <div className="phone-actions">
          <button
            type="button"
            className="clone-button"
            disabled={device.problem !== null}
            onClick={() => onClone(device)}
          >
            <Icon name="copy" size={17} />
            Clone this device
          </button>
          <button
            type="button"
            className="link-button"
            disabled={device.problem !== null}
            onClick={() => onRestore(device)}
          >
            Restore from a clone…
          </button>
        </div>
      )}
    </div>
  );
}
