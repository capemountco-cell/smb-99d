import { DEVICE_GLYPH, count } from "../lib/format";
import type { Device, Progress } from "../lib/native";

type Stage =
  | { kind: "idle" }
  | { kind: "scanning"; progress: Progress | null }
  | { kind: "done" }
  | { kind: "failed"; message: string };

interface Props {
  device: Device | null;
  stage: Stage;
  onScan: (device: Device) => void;
  onCancel: () => void;
}

const WHAT_HAPPENS: Record<Device["kind"], string> = {
  drive:
    "Banker reads the drive without changing it, and lists only what was deleted or is damaged.",
  folder: "Banker reads the folder without changing it, and lists only what was deleted or is damaged.",
  backup: "Banker reads this backup and lists only what was deleted or is damaged.",
  iphone:
    "Banker first backs the iPhone up to this Mac (keep it unlocked), then lists only what was deleted or is damaged.",
  android:
    "Banker first copies the phone to this Mac, then lists only what was deleted or is damaged.",
};

export function ScanPanel({ device, stage, onScan, onCancel }: Props) {
  if (stage.kind === "scanning") {
    const p = stage.progress;
    return (
      <div className="center">
        <div className="spinner" aria-hidden />
        <h2>{p?.phase ?? "Starting…"}</h2>
        {p && p.files > 0 && (
          <p className="muted">
            {count(p.files, "file")} checked · {count(p.findings, "finding")} so far
          </p>
        )}
        {p?.current && <p className="muted path">{p.current}</p>}
        <button type="button" className="ghost" onClick={onCancel}>
          Stop
        </button>
      </div>
    );
  }

  if (!device) {
    return (
      <div className="center">
        <div className="hero-glyph" aria-hidden>
          🔍
        </div>
        <h2>Find what was deleted</h2>
        <p className="muted narrow">
          Connect a drive, an iPhone or an Android phone and pick it on the left. One press
          finds deleted photos, messages and files, and anything damaged, laid out on a
          timeline.
        </p>
      </div>
    );
  }

  return (
    <div className="center">
      <div className="hero-glyph" aria-hidden>
        {DEVICE_GLYPH[device.kind]}
      </div>
      <h2>{device.name}</h2>
      <p className="muted narrow">{WHAT_HAPPENS[device.kind]}</p>
      {device.problem && <p className="warn-text narrow">{device.problem}</p>}
      {stage.kind === "failed" && <p className="error narrow selectable">{stage.message}</p>}
      <button
        type="button"
        className="scan-button"
        disabled={device.problem !== null}
        onClick={() => onScan(device)}
      >
        Scan
      </button>
    </div>
  );
}
