import { useEffect, useState } from "react";

import { bytes, count, DEVICE_ICON } from "../lib/format";
import { listClones, reveal, type CloneEntry, type Device, type Location } from "../lib/native";
import { Icon } from "./Icon";
import { SaveTo } from "./SaveTo";

/** Every clone and Finder backup on this Mac, with what can be done with each. */
export function Backups({
  location,
  onLocation,
  finderBackups,
  onScan,
  onTransfer,
}: {
  location: Location | null;
  onLocation: (location: Location) => void;
  finderBackups: Device[];
  onScan: (device: Device) => void;
  onTransfer: (clone: CloneEntry) => void;
}) {
  const [clones, setClones] = useState<CloneEntry[] | null>(null);
  useEffect(() => {
    listClones()
      .then(setClones)
      .catch(() => setClones([]));
  }, []);

  return (
    <div className="page">
      <div className="headline">
        <h1 className="display">Backups</h1>
        <p className="lede">Clones made with Reco and iPhone backups Finder made.</p>
      </div>
      <SaveTo location={location} onChange={onLocation} />

      {clones !== null && clones.length === 0 && finderBackups.length === 0 && (
        <p className="empty">Nothing yet. Choose a phone in the switcher at the top right and use Clone this phone.</p>
      )}

      <div className="backup-list">
        {(clones ?? []).map((c) => (
          <section key={c.folder} className="backup">
            <span className="tile">
              <Icon name={DEVICE_ICON[c.info.kind]} size={20} />
            </span>
            <div className="backup-text">
              <h2>{c.info.deviceName} clone</h2>
              <p className="muted small-text">
                {new Date(c.info.created * 1000).toLocaleString(undefined, {
                  dateStyle: "medium",
                  timeStyle: "short",
                })}
                {" · "}
                {bytes(c.info.bytes)} · {count(c.info.photos, "photo")}
                {c.info.encrypted && " · password"}
                {c.zipPresent && " · .zip"}
                {!c.backupPresent && " · only the .zip is left"}
              </p>
            </div>
            <div className="row">
              <button type="button" className="ghost small" onClick={() => void reveal(c.folder)}>
                Show in Finder
              </button>
              {c.scanPath && (
                <button
                  type="button"
                  className="ghost small"
                  onClick={() =>
                    onScan({
                      id: `clone:${c.scanPath}`,
                      kind: "backup",
                      name: `${c.info.deviceName} (clone)`,
                      detail: c.scanPath!,
                      path: c.scanPath,
                      problem: null,
                    })
                  }
                >
                  Look for deleted items
                </button>
              )}
              <button type="button" className="primary small" onClick={() => onTransfer(c)}>
                Transfer to a phone
              </button>
            </div>
          </section>
        ))}
        {finderBackups.map((d) => (
          <section key={d.id} className="backup">
            <span className="tile">
              <Icon name="folder" size={20} />
            </span>
            <div className="backup-text">
              <h2>{d.name}</h2>
              <p className="muted small-text">{d.detail} · made by Finder</p>
            </div>
            <div className="row">
              <button type="button" className="primary small" onClick={() => onScan(d)}>
                Look for deleted items
              </button>
            </div>
          </section>
        ))}
      </div>
    </div>
  );
}
