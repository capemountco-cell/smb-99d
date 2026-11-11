import { useState } from "react";

import { DEVICE_ICON } from "../lib/format";
import type { Device, Discovery } from "../lib/native";
import { Icon } from "./Icon";

interface Props {
  discovery: Discovery;
  looking: boolean;
  selected: Device | null;
  /** The device being scanned: amber. */
  scanningId: string | null;
  /** The device whose results are on screen and include deletions: red. */
  foundId: string | null;
  /** Icons only, to give results room. */
  compact: boolean;
  disabled: boolean;
  onSelect: (device: Device) => void;
  onRefresh: () => void;
}

export function Sidebar(props: Props) {
  const { discovery, looking, selected, scanningId, foundId, compact, disabled, onSelect, onRefresh } =
    props;
  const [folder, setFolder] = useState("");
  const connected = discovery.devices.filter((d) => d.kind !== "backup");
  const backups = discovery.devices.filter((d) => d.kind === "backup");

  const marker = (d: Device) => {
    if (d.id === scanningId) return <span className="dot working" title="Scanning" />;
    if (d.problem) return <span className="dot waiting" title={d.problem} />;
    if (d.id === foundId) return <span className="dot deleted" title="Deleted items found" />;
    return null;
  };

  const row = (d: Device) => {
    const state = d.id === scanningId ? "working" : selected?.id === d.id ? "selected" : "";
    return (
      <li key={d.id}>
        <button
          type="button"
          className={`device ${state}`}
          disabled={disabled && d.id !== scanningId}
          onClick={() => onSelect(d)}
          aria-label={compact ? d.name : undefined}
          title={compact ? d.name : undefined}
        >
          <Icon name={DEVICE_ICON[d.kind]} size={20} />
          {!compact && (
            <span className="device-text">
              <span className="device-name">{d.name}</span>
              <span className="device-detail">
                {d.id === scanningId ? "Scanning…" : (d.problem ?? d.detail)}
              </span>
            </span>
          )}
          {marker(d)}
        </button>
      </li>
    );
  };

  return (
    <aside className={`sidebar ${compact ? "compact" : ""}`}>
      <header className="brand">
        <span className="brand-mark">
          <Icon name="search" size={16} stroke={2.2} />
        </span>
        {!compact && <span className="brand-name">Banker</span>}
        {!compact && (
          <button
            type="button"
            className="icon-button"
            onClick={onRefresh}
            disabled={looking || disabled}
            aria-label="Look for devices again"
            title="Look for devices again"
          >
            <Icon name="refresh" size={15} />
          </button>
        )}
      </header>

      <section>
        {!compact && <h2>Connected</h2>}
        {connected.length === 0 ? (
          !compact && (
            <p className="muted small-text pad">
              Plug in a drive, iPhone or Android phone with a cable. It appears here.
            </p>
          )
        ) : (
          <ul className="devices">{connected.map(row)}</ul>
        )}
      </section>

      {backups.length > 0 && (
        <section>
          {!compact && <h2>Backups on this Mac</h2>}
          <ul className="devices">{backups.map(row)}</ul>
        </section>
      )}

      {!compact && (
        <section>
          <h2>A folder</h2>
          <form
            className="folder-form"
            onSubmit={(e) => {
              e.preventDefault();
              const path = folder.trim();
              if (!path) return;
              onSelect({
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
        </section>
      )}

      {!compact && discovery.hints.length > 0 && (
        <section className="hints">
          <h2>Not seeing a device?</h2>
          {discovery.hints.map((h) => (
            <div key={h.text} className="hint">
              <p className="small-text muted">{h.text}</p>
              {h.command && <code className="selectable">{h.command}</code>}
            </div>
          ))}
        </section>
      )}

      <footer className="sidebar-foot">
        {!compact && <span>Reads only. Nothing on a device is changed.</span>}
        <span className="faint">{compact ? "0.1" : "Version 0.1.0"}</span>
      </footer>
    </aside>
  );
}
