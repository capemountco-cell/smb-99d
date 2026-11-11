import { useEffect, useState } from "react";

import { bytes } from "../lib/format";
import {
  listClones,
  reveal,
  type CloneEntry,
  type CloneOptions,
  type Device,
} from "../lib/native";
import { Icon } from "./Icon";

function Sheet({
  title,
  onClose,
  children,
}: {
  title: string;
  onClose: () => void;
  children: React.ReactNode;
}) {
  useEffect(() => {
    const key = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [onClose]);
  return (
    <div className="overlay center-overlay" onClick={onClose} role="presentation">
      <div className="sheet" role="dialog" aria-modal aria-label={title} onClick={(e) => e.stopPropagation()}>
        <header className="sheet-head">
          <h2>{title}</h2>
          <button type="button" className="ghost small" onClick={onClose}>
            Cancel
          </button>
        </header>
        {children}
      </div>
    </div>
  );
}

/** What a clone holds, before it starts. */
export function CloneSheet({
  device,
  onStart,
  onClose,
}: {
  device: Device;
  onStart: (options: CloneOptions) => void;
  onClose: () => void;
}) {
  const iphone = device.kind === "iphone";
  const [complete, setComplete] = useState(true);
  const [password, setPassword] = useState("");
  const [again, setAgain] = useState("");
  const [zip, setZip] = useState(true);
  const needsPassword = iphone && complete;
  const mismatch = needsPassword && again.length > 0 && password !== again;
  const ready = !needsPassword || (password.length >= 4 && password === again);

  return (
    <Sheet title={`Clone ${device.name}`} onClose={onClose}>
      <p className="lede">
        {iphone
          ? "Copies everything on the iPhone to this Mac in Apple's own backup format, so it can restore this iPhone, or a new one, completely. You also get its photos, videos and files sorted into folders."
          : "Copies every photo, video, download, document and app file on the phone to this Mac, sorted into folders, and keeps a copy that can be put back on the phone."}
      </p>
      <ul className="clone-contents">
        <li>
          <Icon name="folder" size={16} />
          <span>
            <strong>Restore backup</strong>
            {iphone ? " · restores the whole iPhone" : " · copies the files back to the phone"}
          </span>
        </li>
        <li>
          <Icon name="photo" size={16} />
          <span>
            <strong>Photos &amp; Videos</strong> · by year and month
          </span>
        </li>
        <li>
          <Icon name="message" size={16} />
          <span>
            <strong>Apps</strong> · each app's photos, videos, voice notes and documents
          </span>
        </li>
      </ul>

      {iphone && (
        <fieldset className="option">
          <label className="check">
            <input type="checkbox" checked={complete} onChange={(e) => setComplete(e.target.checked)} />
            <span>
              <strong>Include saved passwords, Health, Wi-Fi and call history</strong>
              <span className="muted small-text">
                Apple only backs these up with a backup password. The iPhone keeps using this
                password for every backup, Finder's too. Without it they cannot be restored.
              </span>
            </span>
          </label>
          {complete && (
            <div className="password-row">
              <input
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder="Backup password"
                aria-label="Backup password"
                autoComplete="new-password"
              />
              <input
                type="password"
                value={again}
                onChange={(e) => setAgain(e.target.value)}
                placeholder="Again"
                aria-label="Backup password again"
                autoComplete="new-password"
              />
            </div>
          )}
          {mismatch && <p className="small-text deleted-text">The two passwords are different.</p>}
          {complete && (
            <p className="small-text muted">
              Write it down. Nobody, Apple included, can restore this clone without it. If this iPhone
              already has a backup password, use that one.
            </p>
          )}
        </fieldset>
      )}

      <label className="check">
        <input type="checkbox" checked={zip} onChange={(e) => setZip(e.target.checked)} />
        <span>
          <strong>Also make a .zip</strong>
          <span className="muted small-text">
            The restorable copy in one file, to keep somewhere safe. Banker can restore from it.
            Needs about as much space again.
          </span>
        </span>
      </label>

      {!iphone && (
        <p className="small-text muted">
          Android only lets files be copied. Apps, settings and messages cannot be cloned without
          rooting the phone.
        </p>
      )}

      <button
        type="button"
        className="primary wide-button"
        disabled={!ready}
        onClick={() => onStart({ password: needsPassword ? password : null, makeZip: zip })}
      >
        Clone this device
      </button>
    </Sheet>
  );
}

/** Pick a clone and put it back on a phone. On an iPhone this erases it first. */
export function RestoreSheet({
  device,
  onStart,
  onClose,
}: {
  device: Device;
  onStart: (clone: CloneEntry, password: string | null) => void;
  onClose: () => void;
}) {
  const [clones, setClones] = useState<CloneEntry[] | null>(null);
  const [picked, setPicked] = useState<CloneEntry | null>(null);
  const [password, setPassword] = useState("");
  const [sure, setSure] = useState(false);
  const iphone = device.kind === "iphone";

  useEffect(() => {
    listClones()
      .then((all) => {
        const usable = all.filter((c) => c.info.kind === device.kind && (c.backupPresent || c.zipPresent));
        setClones(usable);
        setPicked(usable[0] ?? null);
      })
      .catch(() => setClones([]));
  }, [device.kind]);

  const ready = picked && sure && (!picked.info.encrypted || password.length > 0);

  return (
    <Sheet title={`Restore ${device.name}`} onClose={onClose}>
      {clones === null ? (
        <p className="muted">Looking for clones…</p>
      ) : clones.length === 0 ? (
        <p className="lede">
          There are no {iphone ? "iPhone" : "Android"} clones on this Mac yet. Use “Clone this
          device” first.
        </p>
      ) : (
        <>
          <ul className="clone-list" role="radiogroup" aria-label="Clones">
            {clones.map((c) => (
              <li key={c.folder}>
                <label className={`clone-choice ${picked?.folder === c.folder ? "on" : ""}`}>
                  <input
                    type="radio"
                    name="clone"
                    checked={picked?.folder === c.folder}
                    onChange={() => setPicked(c)}
                  />
                  <span className="clone-choice-text">
                    <strong>{c.info.deviceName}</strong>
                    <span className="muted small-text">
                      {new Date(c.info.created * 1000).toLocaleString(undefined, {
                        dateStyle: "medium",
                        timeStyle: "short",
                      })}
                      {" · "}
                      {bytes(c.info.bytes)}
                      {c.info.osVersion && ` · iOS ${c.info.osVersion}`}
                      {!c.backupPresent && " · from the .zip"}
                      {c.info.encrypted && " · password"}
                    </span>
                  </span>
                </label>
              </li>
            ))}
          </ul>

          {picked?.info.encrypted && (
            <input
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder="This clone's backup password"
              aria-label="Backup password"
            />
          )}

          {iphone ? (
            <div className="warning">
              <p>
                <strong>This erases {device.name}</strong> and replaces everything on it with the
                clone. Before you start:
              </p>
              <ul>
                <li>Turn off Find My on the iPhone (Settings › your name › Find My).</li>
                <li>Keep it plugged in and unlocked until it restarts.</li>
                <li>Apps download again from the App Store, with their data.</li>
              </ul>
            </div>
          ) : (
            <p className="small-text muted">
              Copies the clone's files back onto the phone. Nothing on the phone is deleted; files
              with the same name are replaced.
            </p>
          )}

          <label className="check">
            <input type="checkbox" checked={sure} onChange={(e) => setSure(e.target.checked)} />
            <span>
              {iphone
                ? `I understand ${device.name} will be erased.`
                : "Copy the files back to this phone."}
            </span>
          </label>

          <button
            type="button"
            className={`${iphone ? "danger" : "primary"} wide-button`}
            disabled={!ready}
            onClick={() => picked && onStart(picked, picked.info.encrypted ? password : null)}
          >
            {iphone ? `Erase and restore ${device.name}` : "Copy files back"}
          </button>
        </>
      )}
    </Sheet>
  );
}

/** Green: the clone is safely on this Mac. */
export function CloneDone({
  clone,
  onScan,
  onBack,
}: {
  clone: CloneEntry;
  onScan: (path: string) => void;
  onBack: () => void;
}) {
  const i = clone.info;
  return (
    <div className="hero">
      <span className="state-pill saved">
        <Icon name="check" size={14} stroke={2.4} />
        Saved
      </span>
      <h1 className="display">{i.deviceName} is cloned</h1>
      <p className="lede narrow">
        {bytes(i.bytes)} copied to this Mac
        {i.kind === "iphone" ? ", ready to restore this iPhone or a new one." : "."}
      </p>
      <div className="clone-stats">
        <div>
          <strong>{i.photos.toLocaleString()}</strong>
          <span className="muted small-text">photos and videos</span>
        </div>
        <div>
          <strong>{i.appFiles.toLocaleString()}</strong>
          <span className="muted small-text">app files</span>
        </div>
        <div>
          <strong>{i.otherFiles.toLocaleString()}</strong>
          <span className="muted small-text">other files</span>
        </div>
      </div>
      {i.zip && <p className="muted small-text">Also saved as {i.zip}</p>}
      {i.notes.map((n) => (
        <p key={n} className="banner narrow-banner">
          {n}
        </p>
      ))}
      <div className="row center-row">
        <button type="button" className="primary" onClick={() => void reveal(clone.folder)}>
          Show in Finder
        </button>
        {clone.scanPath && (
          <button type="button" className="ghost" onClick={() => onScan(clone.scanPath!)}>
            Look for deleted items in it
          </button>
        )}
        <button type="button" className="ghost" onClick={onBack}>
          Done
        </button>
      </div>
      <p className="muted path selectable">{clone.folder}</p>
    </div>
  );
}

export function RestoreDone({ message, onBack }: { message: string; onBack: () => void }) {
  return (
    <div className="hero">
      <span className="state-pill saved">
        <Icon name="check" size={14} stroke={2.4} />
        Restored
      </span>
      <h1 className="display">Restore sent to the phone</h1>
      <p className="lede narrow">{message}</p>
      <button type="button" className="ghost" onClick={onBack}>
        Done
      </button>
    </div>
  );
}
