import { useState } from "react";

import { DEVICE_GLYPH } from "../lib/format";
import type { Device, Discovery } from "../lib/native";

interface Props {
  discovery: Discovery;
  looking: boolean;
  selected: Device | null;
  disabled: boolean;
  onSelect: (device: Device) => void;
  onRefresh: () => void;
}

export function Sidebar({ discovery, looking, selected, disabled, onSelect, onRefresh }: Props) {
  const [folder, setFolder] = useState("");
  const connected = discovery.devices.filter((d) => d.kind !== "backup");
  const backups = discovery.devices.filter((d) => d.kind === "backup");

  const row = (d: Device) => (
    <li key={d.id}>
      <button
        type="button"
        className={`device ${selected?.id === d.id ? "selected" : ""}`}
        disabled={disabled}
        onClick={() => onSelect(d)}
      >
        <span className="glyph" aria-hidden>
          {DEVICE_GLYPH[d.kind]}
        </span>
        <span className="device-text">
          <span className="device-name">{d.name}</span>
          <span className="device-detail">{d.problem ?? d.detail}</span>
        </span>
        {d.problem && <span className="dot warn" title={d.problem} />}
      </button>
    </li>
  );

  return (
    <aside className="sidebar">
      <header className="sidebar-head">
        <h1>Banker</h1>
        <button
          type="button"
          className="ghost small"
          onClick={onRefresh}
          disabled={looking || disabled}
          title="Look for devices again"
        >
          {looking ? "Looking…" : "Refresh"}
        </button>
      </header>

      <section>
        <h2>Connected</h2>
        {connected.length === 0 ? (
          <p className="muted small-text">
            Plug in a drive, iPhone or Android phone with a cable. It appears here.
          </p>
        ) : (
          <ul className="devices">{connected.map(row)}</ul>
        )}
      </section>

      {backups.length > 0 && (
        <section>
          <h2>iPhone backups on this Mac</h2>
          <ul className="devices">{backups.map(row)}</ul>
        </section>
      )}

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
            placeholder="/Volumes/Card or ~/Backups"
            aria-label="Folder to scan"
            disabled={disabled}
          />
          <button type="submit" className="ghost small" disabled={disabled}>
            Use
          </button>
        </form>
      </section>

      {discovery.hints.length > 0 && (
        <section className="hints">
          <h2>To see more devices</h2>
          {discovery.hints.map((h) => (
            <div key={h.tool} className="hint">
              <p className="small-text">
                Install {h.tool} {h.why}:
              </p>
              <code className="selectable">{h.install}</code>
            </div>
          ))}
        </section>
      )}
    </aside>
  );
}
