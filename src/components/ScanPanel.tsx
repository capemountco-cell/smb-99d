import { DEVICE_ICON, count } from "../lib/format";
import type { Device, Progress } from "../lib/native";
import { Icon, type IconName } from "./Icon";

export type Stage =
  | { kind: "idle" }
  | { kind: "scanning"; device: Device; progress: Progress | null }
  | { kind: "failed"; message: string };

interface Props {
  device: Device | null;
  stage: Stage;
  onScan: (device: Device) => void;
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

/** The phones copy first, so the scan has two visible steps. */
function steps(device: Device, phase: string) {
  const copies = device.kind === "iphone" || device.kind === "android";
  const copying = copies && /back|copy/i.test(phase);
  const list = [
    ...(copies ? [{ label: "Copy to this Mac", state: copying ? "now" : "done" }] : []),
    { label: "Look for deleted and damaged items", state: copying ? "next" : "now" },
  ];
  return list as { label: string; state: "done" | "now" | "next" }[];
}

export function ScanPanel({ device, stage, onScan, onCancel }: Props) {
  if (stage.kind === "scanning") {
    const p = stage.progress;
    const phase = p?.phase ?? "Starting…";
    return (
      <div className="hero working-view">
        <span className="state-pill working">
          <span className="dot working" />
          Working
        </span>
        <h1 className="display">Reading {stage.device.name}</h1>
        <p className="lede">{phase}. You can keep using your Mac.</p>
        <div className="ring" role="progressbar" aria-label={phase}>
          <div className="ring-inner">
            <span className="ring-number">{p ? p.files.toLocaleString() : "…"}</span>
            <span className="muted small-text">files read</span>
          </div>
        </div>
        <ol className="steps">
          {steps(stage.device, phase).map((s) => (
            <li key={s.label} className={`step ${s.state}`}>
              {s.state === "done" ? (
                <Icon name="check" size={16} stroke={2.2} />
              ) : (
                <span className="step-mark" />
              )}
              {s.label}
            </li>
          ))}
        </ol>
        <div className="row center-row">
          {p && p.findings > 0 && (
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
    </div>
  );
}
