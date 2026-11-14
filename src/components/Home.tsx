import { bytes, count, DEVICE_ICON } from "../lib/format";
import type { Device, Location, ScanSummary } from "../lib/native";
import { Icon } from "./Icon";
import type { Working } from "./ScanPanel";

/** What a scan of this Mac reads, in order. Matches MAC_PARTS in scan.rs. */
const MAC_PARTS = [
  "Photos · Recently Deleted",
  "Trash",
  "Messages and notes",
  "Desktop, Documents and Downloads",
];

const isPhone = (d: Device) => d.kind === "iphone" || d.kind === "android";

/**
 * Overview: this Mac first, then the drives and phones plugged in. A Mac or
 * a drive is read where it is, so scanning it needs no space.
 */
export function Home({
  devices,
  location,
  last,
  failed,
  onScan,
  onOpenPhone,
  onSeeFound,
}: {
  devices: Device[];
  location: Location | null;
  last: { device: Device; summary: ScanSummary } | null;
  failed: string | null;
  onScan: (device: Device) => void;
  onOpenPhone: (device: Device) => void;
  onSeeFound: () => void;
}) {
  const mac = devices.find((d) => d.kind === "mac");
  const drives = devices.filter((d) => d.kind === "drive");
  const phones = devices.filter(isPhone);
  const volume = mac?.detail.split(" · ")[0] ?? "Macintosh HD";

  return (
    <div className="overview-page">
      <div className="overview-hero">
        <h1 className="display">Get back what was deleted</h1>
        <p className="lede">
          Reco reads your Mac and your drives where they are. Nothing is copied while it looks:
          space is used only for the photos and files you choose to recover.
        </p>
      </div>
      {failed && <p className="banner error selectable">{failed}</p>}
      {last && last.summary.findings > 0 && (
        <button type="button" className="last-scan" onClick={onSeeFound}>
          <span>
            <strong>{count(last.summary.findings, "deleted or damaged item")}</strong>
            <span className="muted"> found on {last.device.name}</span>
          </span>
          <span className="muted">Look through them ›</span>
        </button>
      )}

      <div className="overview-grid">
        {mac && (
          <section className="ov-card ov-mac">
            <div className="ov-head">
              <div>
                <h2>This Mac</h2>
                <p className="muted">
                  {volume}
                  {location?.label === "This Mac" && location.freeBytes !== null
                    ? ` · ${bytes(location.freeBytes)} free`
                    : ""}
                </p>
              </div>
              <span className="ov-tile">
                <Icon name="mac" size={22} />
              </span>
            </div>
            <ul className="ov-list">
              <li>
                <span>Photos library · Recently Deleted</span>
                <span className="muted">read in place</span>
              </li>
              <li>
                <span>The Trash</span>
                <span className="muted">read in place</span>
              </li>
              <li>
                <span>Deleted messages and notes</span>
                <span className="muted">read in place</span>
              </li>
              <li>
                <span>Damaged photos and documents</span>
                <span className="muted">read in place</span>
              </li>
            </ul>
            <div className="ov-foot">
              <button type="button" className="primary big" onClick={() => onScan(mac)}>
                <Icon name="search" size={16} stroke={2.2} />
                Scan this Mac
              </button>
              <span className="muted">No space needed</span>
            </div>
          </section>
        )}

        {drives.map((d) => (
          <section key={d.id} className="ov-card">
            <div className="ov-head">
              <h2>{d.name}</h2>
              <span className="muted small-text">Drive</span>
            </div>
            <p className="muted small-text">Its trash and damaged files, read in place. Nothing on it changes.</p>
            <button type="button" className="secondary ov-action" disabled={!!d.problem} onClick={() => onScan(d)}>
              Scan
            </button>
          </section>
        ))}

        {drives.length === 0 && (
          <section className="ov-card quiet">
            <div className="ov-head">
              <h2>Drives and cards</h2>
              <Icon name="drive" size={18} />
            </div>
            <p className="muted small-text">
              Plug in a USB drive or a camera card. It shows up here, and is read where it is.
            </p>
          </section>
        )}

        {phones.length > 0 ? (
          phones.map((d) => (
            <section key={d.id} className="ov-card ov-wide">
              <span className="ov-tile small">
                <Icon name={DEVICE_ICON[d.kind]} size={18} />
              </span>
              <div className="ov-wide-text">
                <strong>{d.name}</strong>
                <span className="muted small-text">
                  {d.problem ?? "Scan its deleted photos without copying the whole phone."}
                </span>
              </div>
              <button type="button" className="secondary ov-action" onClick={() => onOpenPhone(d)}>
                Open
              </button>
            </section>
          ))
        ) : (
          <section className="ov-card ov-wide quiet">
            <span className="ov-tile small">
              <Icon name="iphone" size={18} />
            </span>
            <div className="ov-wide-text">
              <strong>Phones</strong>
              <span className="muted small-text">Plug in an iPhone or Android phone with a cable. It is found by itself.</span>
            </div>
          </section>
        )}
      </div>
    </div>
  );
}

/**
 * A scan in progress. This Mac and drives are read where they are, so the
 * space used stays at zero; that is said plainly while it runs.
 */
export function ScanningView({ stage, onCancel }: { stage: Extract<Working, { kind: "scanning" }>; onCancel: () => void }) {
  const p = stage.progress;
  const phase = p?.phase ?? "Starting";
  const d = stage.device;
  const mac = d.kind === "mac";
  const inPlace = mac || d.kind === "drive" || d.kind === "folder" || d.kind === "backup";
  const at = mac ? Math.max(0, MAC_PARTS.indexOf(phase)) : -1;
  const percent = mac ? Math.round((at / MAC_PARTS.length) * 100) : (p?.percent ?? null);

  return (
    <div className="scanning-page">
      <div className="scanning-main">
        <span className="state-line">
          <span className="dot accent" />
          Scanning
        </span>
        <h1 className="display">{mac ? "Looking through this Mac" : `Looking through ${d.name}`}</h1>
        <p className="lede">
          {inPlace
            ? "Reading in place. Nothing is being copied, so this uses no space."
            : stage.depth === "photos"
              ? "Only the deleted photos are fetched from the phone, so this needs almost no space."
              : "Copying the phone once so its messages and app data can be read. The copy is reused next time."}
        </p>
        <div className="bar" role="progressbar" aria-label={phase} aria-valuenow={percent ?? undefined}>
          <div className={`bar-fill ${percent === null ? "unknown" : ""}`} style={{ width: `${percent ?? 30}%` }} />
        </div>
        <p className="muted small-text">{phase === "Starting" ? "Starting…" : phase}</p>

        {mac && (
          <ul className="part-list">
            {MAC_PARTS.map((label, i) => (
              <li key={label}>
                <span>{label}</span>
                {i < at ? (
                  <span className="ok-text">✓ done</span>
                ) : i === at ? (
                  <span className="muted">reading…</span>
                ) : (
                  <span className="faint">next</span>
                )}
              </li>
            ))}
          </ul>
        )}
        <div className="row">
          <button type="button" className="ghost" onClick={onCancel}>
            Stop
          </button>
        </div>
      </div>

      <aside className="scanning-side">
        <div className="big-stat">
          <strong>{(p?.findings ?? 0).toLocaleString()}</strong>
          <span className="muted">found so far</span>
        </div>
        <div className="big-stat">
          <strong>{(p?.files ?? 0).toLocaleString()}</strong>
          <span className="muted">files looked at</span>
        </div>
        {inPlace && (
          <div className="big-stat">
            <strong>0 bytes</strong>
            <span className="muted">space used</span>
          </div>
        )}
        {p?.current && <p className="faint small-text path">{p.current}</p>}
      </aside>
    </div>
  );
}
