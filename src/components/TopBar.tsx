import { useEffect, useState } from "react";

import { DEVICE_ICON } from "../lib/format";
import type { Device, Discovery } from "../lib/native";
import { Icon } from "./Icon";

export type Section = "scan" | "clone" | "transfer" | "restore" | "backups";

const SECTIONS: { id: Section; label: string }[] = [
  { id: "scan", label: "Scan" },
  { id: "clone", label: "Clone" },
  { id: "transfer", label: "Transfer" },
  { id: "restore", label: "Restore" },
  { id: "backups", label: "Backups" },
];

interface Props {
  discovery: Discovery;
  looking: boolean;
  selected: Device | null;
  active: Section | "devices" | null;
  /** The device Banker is working on, and what it is doing: amber. */
  working: { id: string; label: string } | null;
  /** The device whose results are on screen and include deletions: red. */
  foundId: string | null;
  disabled: boolean;
  /** Sections that make no sense for the selected device right now. */
  unavailable: Set<Section>;
  onSection: (section: Section) => void;
  onSelect: (device: Device) => void;
  onRefresh: () => void;
}

/**
 * The global navigation: one thin frosted bar across the top, sections in
 * the middle, the current device on the right. Devices open as a
 * full-width menu that drops down from the bar.
 */
export function TopBar(props: Props) {
  const { selected, active, working, foundId, disabled, unavailable, onSection, onSelect } = props;
  const [open, setOpen] = useState(false);

  useEffect(() => {
    if (!open) return;
    const key = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [open]);

  const pillState =
    working && selected?.id === working.id
      ? "working"
      : selected && selected.id === foundId
        ? "deleted"
        : selected?.problem
          ? "waiting"
          : null;

  return (
    <>
      <header className={`globalnav ${open ? "open" : ""}`}>
        <button type="button" className="brand" onClick={() => onSection("scan")}>
          <span className="brand-mark">
            <Icon name="search" size={12} stroke={2.6} />
          </span>
          Banker
        </button>
        <nav aria-label="Main" className="nav-links">
          <button
            type="button"
            className={active === "devices" || open ? "on" : ""}
            aria-expanded={open}
            onClick={() => setOpen(!open)}
          >
            Devices
          </button>
          {SECTIONS.map((s) => (
            <button
              key={s.id}
              type="button"
              className={active === s.id && !open ? "on" : ""}
              disabled={disabled || unavailable.has(s.id)}
              onClick={() => {
                setOpen(false);
                onSection(s.id);
              }}
            >
              {s.label}
            </button>
          ))}
        </nav>
        <div className="nav-end">
          <button
            type="button"
            className={`device-pill ${pillState ?? ""}`}
            aria-expanded={open}
            onClick={() => setOpen(!open)}
          >
            {pillState === "working" || pillState === "deleted" ? (
              <span className={`dot ${pillState}`} />
            ) : (
              <Icon name={selected ? DEVICE_ICON[selected.kind] : "search"} size={14} stroke={2} />
            )}
            {selected
              ? `${selected.name}${pillState === "working" && working ? ` · ${working.label}` : ""}`
              : "Choose a device"}
            <Icon name={open ? "up" : "down"} size={10} stroke={2.6} />
          </button>
        </div>
      </header>
      {open && (
        <>
          <DevicesMenu
            {...props}
            onPick={(d) => {
              onSelect(d);
              setOpen(false);
            }}
          />
          <div className="menu-scrim" onClick={() => setOpen(false)} role="presentation" />
        </>
      )}
    </>
  );
}

function DevicesMenu({
  discovery,
  looking,
  selected,
  working,
  disabled,
  onRefresh,
  onPick,
}: Props & { onPick: (d: Device) => void }) {
  const [folder, setFolder] = useState("");
  const connected = discovery.devices.filter((d) => d.kind !== "backup");
  const backups = discovery.devices.filter((d) => d.kind === "backup");

  return (
    <section className="devices-menu" aria-label="Devices">
      <div className="menu-col wide">
        <span className="menu-label">
          Connected
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
        </span>
        {connected.length === 0 && (
          <p className="muted">Plug in a drive, iPhone or Android phone with a cable. It appears here.</p>
        )}
        {connected.map((d) => (
          <button
            key={d.id}
            type="button"
            className={`menu-device ${selected?.id === d.id ? "current" : ""}`}
            disabled={disabled && d.id !== working?.id}
            onClick={() => onPick(d)}
          >
            <Icon name={DEVICE_ICON[d.kind]} size={22} stroke={1.7} />
            <span className="menu-device-text">
              <span className="menu-device-name">{d.name}</span>
              <span className="small-text muted">
                {d.id === working?.id ? working.label : (d.problem ?? d.detail)}
                {selected?.id === d.id ? " · selected" : ""}
              </span>
            </span>
            {d.id === working?.id ? (
              <span className="dot working" />
            ) : d.problem ? (
              <span className="dot waiting" title={d.problem} />
            ) : null}
          </button>
        ))}
      </div>

      <div className="menu-col">
        <span className="menu-label">Backups on this Mac</span>
        {backups.length === 0 && <p className="small-text muted">None yet.</p>}
        {backups.map((d) => (
          <button
            key={d.id}
            type="button"
            className="menu-link"
            disabled={disabled}
            onClick={() => onPick(d)}
          >
            {d.name} · {d.detail}
          </button>
        ))}
      </div>

      <div className="menu-col">
        <span className="menu-label">A folder</span>
        <form
          className="folder-form"
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
            disabled={disabled}
          />
          <button type="submit" className="ghost small" disabled={disabled}>
            Use
          </button>
        </form>
        <span className="menu-label spaced">Not seeing a device?</span>
        <p className="small-text muted">Unlock the phone and tap Trust or Allow.</p>
        {discovery.hints.map((h) => (
          <div key={h.text} className="hint">
            <p className="small-text muted">{h.text}</p>
            {h.command && <code className="selectable">{h.command}</code>}
          </div>
        ))}
      </div>
    </section>
  );
}
