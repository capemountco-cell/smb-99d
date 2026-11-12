import { useEffect, useState } from "react";

import { DEVICE_ICON, bytes } from "../lib/format";
import type { CloneEntry, Device, Discovery } from "../lib/native";
import { Icon } from "./Icon";

export type Place = "device" | "results" | "backups";

/** What the switcher says about the current device, and its colour. */
export interface DeviceState {
  tone: "none" | "ready" | "needs" | "working" | "deleted" | "saved";
  line: string;
}

const PLACES: { id: Place; label: string }[] = [
  { id: "device", label: "Device" },
  { id: "results", label: "Results" },
  { id: "backups", label: "Backups" },
];

interface Props {
  discovery: Discovery;
  clones: CloneEntry[];
  looking: boolean;
  selected: Device | null;
  state: DeviceState;
  place: Place;
  /** True while Banker works; switching device waits until it is done. */
  busy: boolean;
  onPlace: (place: Place) => void;
  onSelect: (device: Device) => void;
  onRefresh: () => void;
}

/**
 * The global bar: the Banker mark, three places that are never disabled,
 * and the device switcher, which says what state the device is in.
 */
export function TopBar(props: Props) {
  const { selected, state, place, onPlace } = props;
  const [open, setOpen] = useState(false);

  useEffect(() => {
    if (!open) return;
    const key = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [open]);

  return (
    <>
      <header className="globalnav">
        <button type="button" className="brand" onClick={() => onPlace("device")}>
          <span className="brand-mark" aria-hidden>
            B
          </span>
          Banker
        </button>
        <nav aria-label="Main" className="places">
          {PLACES.map((p) => (
            <button
              key={p.id}
              type="button"
              className={place === p.id ? "current" : ""}
              aria-current={place === p.id ? "page" : undefined}
              onClick={() => onPlace(p.id)}
            >
              {p.label}
            </button>
          ))}
        </nav>
        <button
          type="button"
          className={`switcher tone-${state.tone}`}
          aria-expanded={open}
          aria-haspopup="menu"
          onClick={() => setOpen(!open)}
        >
          <span className="switcher-tile" aria-hidden>
            {state.tone === "ready" || state.tone === "none" ? (
              <Icon name={selected ? DEVICE_ICON[selected.kind] : "search"} size={15} stroke={1.9} />
            ) : (
              <span className="switcher-dot" />
            )}
          </span>
          <span className="switcher-text">
            <span className="switcher-name">{selected?.name ?? "Choose a device"}</span>
            <span className="switcher-line">{state.line}</span>
          </span>
          <Icon name={open ? "up" : "down"} size={12} stroke={2.4} />
        </button>
      </header>
      {open && (
        <>
          <div className="menu-scrim" onClick={() => setOpen(false)} role="presentation" />
          <DeviceMenu
            {...props}
            onPick={(d) => {
              props.onSelect(d);
              setOpen(false);
            }}
            onBackups={() => {
              onPlace("backups");
              setOpen(false);
            }}
          />
        </>
      )}
    </>
  );
}

function DeviceMenu({
  discovery,
  clones,
  looking,
  selected,
  busy,
  onRefresh,
  onPick,
  onBackups,
}: Props & { onPick: (d: Device) => void; onBackups: () => void }) {
  const [folderOpen, setFolderOpen] = useState(false);
  const [folder, setFolder] = useState("");
  const connected = discovery.devices.filter((d) => d.kind !== "backup");
  const finder = discovery.devices.filter((d) => d.kind === "backup");

  return (
    <section className="device-menu" role="menu" aria-label="Devices">
      <div className="menu-head">
        <span className="section-label">Connected</span>
        <button
          type="button"
          className="icon-button"
          onClick={onRefresh}
          disabled={looking}
          aria-label="Look for devices again"
          title="Look for devices again"
        >
          <Icon name="refresh" size={13} />
        </button>
      </div>
      {connected.length === 0 && (
        <p className="menu-empty">Plug in an iPhone, an Android phone or a drive with a cable.</p>
      )}
      {connected.map((d) => {
        const current = selected?.id === d.id;
        return (
          <button
            key={d.id}
            type="button"
            role="menuitem"
            className={`menu-row ${current ? "current" : ""} ${d.problem ? "needs" : ""}`}
            disabled={busy && !current}
            onClick={() => onPick(d)}
          >
            <span className="menu-tile">
              <Icon name={DEVICE_ICON[d.kind]} size={18} />
            </span>
            <span className="menu-text">
              <span className="menu-name">{d.name}</span>
              <span className="menu-line">{d.problem ? `Needs you · ${shortProblem(d.problem)}` : d.detail}</span>
            </span>
            {current ? <Icon name="check" size={16} stroke={2.4} /> : d.problem ? <span className="how">How</span> : null}
          </button>
        );
      })}

      {(clones.length > 0 || finder.length > 0) && (
        <>
          <div className="menu-rule" />
          <span className="section-label menu-pad">On this Mac</span>
          {clones.slice(0, 3).map((c) => (
            <button key={c.folder} type="button" role="menuitem" className="menu-link" onClick={onBackups}>
              <span>
                {c.info.deviceName} clone ·{" "}
                {new Date(c.info.created * 1000).toLocaleDateString(undefined, { day: "numeric", month: "short" })}
              </span>
              <span className="menu-meta">{bytes(c.info.bytes)}</span>
            </button>
          ))}
          {finder.map((d) => (
            <button
              key={d.id}
              type="button"
              role="menuitem"
              className="menu-link"
              disabled={busy}
              onClick={() => onPick(d)}
            >
              <span>{d.name} · Finder backup</span>
            </button>
          ))}
        </>
      )}

      <div className="menu-rule" />
      {folderOpen ? (
        <form
          className="folder-form menu-pad"
          onSubmit={(e) => {
            e.preventDefault();
            const path = folder.trim();
            if (!path) return;
            onPick({
              id: `folder:${path}`,
              kind: "folder",
              name: path.split("/").filter(Boolean).pop() ?? path,
              detail: path,
              path,
              problem: null,
            });
          }}
        >
          <input
            value={folder}
            onChange={(e) => setFolder(e.target.value)}
            placeholder="/Volumes/Card"
            aria-label="Folder to scan"
            autoFocus
          />
          <button type="submit" className="secondary small">
            Use
          </button>
        </form>
      ) : (
        <button type="button" role="menuitem" className="menu-link quiet" onClick={() => setFolderOpen(true)}>
          <span className="menu-link-icon">
            <Icon name="folder" size={16} />
            Scan a folder…
          </span>
        </button>
      )}

      {discovery.hints.length > 0 && (
        <div className="menu-hints">
          {discovery.hints.map((h) => (
            <div key={h.text} className="hint">
              <p className="small-text muted">{h.text}</p>
              {h.command && <code className="selectable">{h.command}</code>}
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

/** The first clause of a problem, short enough for one menu line. */
function shortProblem(problem: string): string {
  const first = problem.split(/[.:]/)[0] ?? problem;
  return first.length > 48 ? `${first.slice(0, 46)}…` : first;
}
