import { useEffect, useMemo, useRef, useState } from "react";

import { MEDIA_ICON, bytes, count, when } from "../lib/format";
import {
  cloneGroups,
  cloneItems,
  cloneThumbnail,
  type CloneEntry,
  type CloneGroup,
  type CloneItem,
  type Device,
  type Selection,
} from "../lib/native";
import { Icon } from "./Icon";

/** "photos and videos", "WhatsApp files", "downloads"... */
function groupNoun(g: CloneGroup): string {
  if (g.id === "Photos & Videos") return g.files === 1 ? "photo or video" : "photos and videos";
  if (g.id.startsWith("Apps/")) return `${g.label} ${g.files === 1 ? "file" : "files"}`;
  const plain: Record<string, string> = {
    "Files on iPhone": "files from Files on iPhone",
    Downloads: "downloads",
    Documents: "documents",
    Audio: "audio files",
  };
  return plain[g.id] ?? g.label;
}

/** The Gallery album transferred photos land in. */
export const albumFor = (clone: CloneEntry) => `From ${clone.info.deviceName}`;

const thumbs = new Map<string, string | null>();

function CloneThumb({ folder, path, className }: { folder: string; path: string; className: string }) {
  const key = `${folder}\n${path}`;
  const ref = useRef<HTMLDivElement>(null);
  const [url, setUrl] = useState<string | null>(thumbs.get(key) ?? null);
  useEffect(() => {
    if (thumbs.has(key) || !ref.current) return;
    const el = ref.current;
    const seen = new IntersectionObserver(([entry]) => {
      if (!entry?.isIntersecting) return;
      seen.disconnect();
      cloneThumbnail(folder, path)
        .then((u) => {
          thumbs.set(key, u);
          setUrl(u);
        })
        .catch(() => thumbs.set(key, null));
    });
    seen.observe(el);
    return () => seen.disconnect();
  }, [key, folder, path]);
  return (
    <div className={className} ref={ref}>
      {url ? <img src={url} alt="" /> : <Icon name="photo" size={20} stroke={1.4} />}
    </div>
  );
}

/** The new phone is plugged in and a clone of another phone is on this Mac. */
export function TransferPrompt({
  device,
  clone,
  onChoose,
  onAll,
  onDismiss,
}: {
  device: Device;
  clone: CloneEntry;
  onChoose: () => void;
  onAll: () => void;
  onDismiss: () => void;
}) {
  const canPick = device.kind === "android";
  return (
    <div className="prompt" role="dialog" aria-label="Transfer cloned files">
      <span className="tile">
        <Icon name={device.kind === "iphone" ? "iphone" : "android"} size={20} />
      </span>
      <div className="prompt-text">
        <strong>{device.name} is connected</strong>
        <span className="muted small-text">
          Transfer files from the clone of {clone.info.deviceName} (
          {new Date(clone.info.created * 1000).toLocaleDateString(undefined, { dateStyle: "medium" })})?
        </span>
      </div>
      <button type="button" className="ghost small" onClick={onDismiss}>
        Not now
      </button>
      {canPick && (
        <button type="button" className="ghost" onClick={onChoose}>
          Choose files
        </button>
      )}
      <button type="button" className="primary" onClick={canPick ? onAll : onChoose}>
        {canPick ? "Transfer all" : "Review"}
      </button>
    </div>
  );
}

/** Waiting for the new phone after "Transfer to another phone". */
export function AwaitingPhone({ clone, onCancel }: { clone: CloneEntry; onCancel: () => void }) {
  return (
    <div className="hero">
      <div className="device-art empty" aria-hidden>
        <Icon name="android" size={56} stroke={1.3} />
      </div>
      <h1 className="display">Connect the new phone</h1>
      <p className="lede narrow">
        Unplug {clone.info.deviceName} and plug in the phone you are moving to. Banker offers the
        transfer as soon as it appears. An Android phone needs USB debugging on.
      </p>
      <button type="button" className="ghost" onClick={onCancel}>
        Cancel
      </button>
    </div>
  );
}

/** Pick what to move from a clone onto the connected phone. */
export function TransferView({
  device,
  clone,
  onTransfer,
  onRestore,
  onBack,
}: {
  device: Device;
  clone: CloneEntry;
  onTransfer: (selection: Selection) => void;
  onRestore: () => void;
  onBack: () => void;
}) {
  const [groups, setGroups] = useState<CloneGroup[] | null>(null);
  const [whole, setWhole] = useState<Set<string>>(new Set());
  const [files, setFiles] = useState<Map<string, number>>(new Map());
  const [browsing, setBrowsing] = useState<CloneGroup | null>(null);
  const [error, setError] = useState<string | null>(null);
  const toIphone = device.kind === "iphone";

  useEffect(() => {
    if (toIphone) return;
    cloneGroups(clone.folder)
      .then(setGroups)
      .catch((e) => setError(String(e)));
  }, [clone.folder, toIphone]);

  const picked = useMemo(() => {
    let n = 0;
    let size = 0;
    for (const g of groups ?? []) {
      if (whole.has(g.id)) {
        n += g.files;
        size += g.bytes;
      }
    }
    for (const [path, s] of files) {
      if (![...whole].some((g) => path.startsWith(`${g}/`))) {
        n += 1;
        size += s;
      }
    }
    return { n, size };
  }, [groups, whole, files]);

  const head = (
    <header className="topbar">
      <button type="button" className="back" onClick={onBack}>
        <Icon name="back" size={16} stroke={2} />
        Back
      </button>
      <span className="muted">Transfer</span>
      <span className="topbar-spacer" />
    </header>
  );

  if (toIphone) {
    const fromIphone = clone.info.kind === "iphone";
    return (
      <div className="overview">
        {head}
        <div className="headline">
          <h1>
            {fromIphone
              ? `Move everything from ${clone.info.deviceName} to ${device.name}`
              : `${clone.info.deviceName} cannot be moved to an iPhone by cable`}
          </h1>
          <p className="lede narrow">
            {fromIphone
              ? "Apple moves an iPhone to another iPhone by restoring the whole phone from the clone: photos, messages, apps' data and settings. It erases what is on the new iPhone first. Apple allows no app to add single files to an iPhone."
              : "Apple only accepts Android phones through its Move to iOS app, which runs on the Android phone during the new iPhone's setup. The clone's organised folders are on this Mac to AirDrop or drag into apps."}
          </p>
          {fromIphone ? (
            <button type="button" className="danger" onClick={onRestore}>
              Erase {device.name} and move everything
            </button>
          ) : null}
        </div>
      </div>
    );
  }

  const total = (groups ?? []).reduce((a, g) => a + g.files, 0);
  const allSelected = groups !== null && groups.length > 0 && groups.every((g) => whole.has(g.id));

  return (
    <div className="overview">
      {head}
      <div className="headline">
        <h1>
          Move {groups ? count(total, "file") : "files"} from {clone.info.deviceName} to {device.name}
        </h1>
        <p className="lede">
          Nothing on {device.name} is removed. Photos and videos appear in the Gallery album “
          {albumFor(clone)}”.
        </p>
        <button
          type="button"
          className="primary"
          disabled={!groups?.length}
          onClick={() => onTransfer({ groups: (groups ?? []).map((g) => g.id), files: [] })}
        >
          Transfer everything
        </button>
      </div>
      {error && <p className="banner error selectable">{error}</p>}

      {groups && groups.length === 0 && (
        <p className="empty">This clone has no organised files to transfer.</p>
      )}

      <div className="groups transfer-groups">
        {(groups ?? []).map((g, i) => {
          const on = whole.has(g.id);
          const some = !on && [...files.keys()].some((p) => p.startsWith(`${g.id}/`));
          return (
            <section key={g.id} className={`group ${i === 0 && g.sample.length ? "tall" : ""} ${on ? "picked" : ""}`}>
              <div className="group-head">
                <div>
                  <h2>
                    {g.files.toLocaleString()} {groupNoun(g)}
                  </h2>
                  <p className="muted">{bytes(g.bytes)}</p>
                </div>
                <span className="tile">
                  <Icon name={MEDIA_ICON[g.media]} size={20} />
                </span>
              </div>
              {g.sample.length > 0 && (
                <div className="mosaic">
                  {g.sample.slice(0, i === 0 ? 6 : 3).map((path) => (
                    <CloneThumb key={path} folder={clone.folder} path={path} className="mosaic-tile neutral" />
                  ))}
                </div>
              )}
              <div className="group-foot">
                <label className="check inline">
                  <input
                    type="checkbox"
                    checked={on}
                    ref={(el) => {
                      if (el) el.indeterminate = some;
                    }}
                    onChange={() => {
                      const next = new Set(whole);
                      if (on) next.delete(g.id);
                      else next.add(g.id);
                      setWhole(next);
                    }}
                  />
                  <span>{on ? "All included" : some ? "Some included" : "Include all"}</span>
                </label>
                <button type="button" className="ghost" onClick={() => setBrowsing(g)}>
                  Choose files
                </button>
              </div>
            </section>
          );
        })}
      </div>

      {groups && groups.length > 0 && (
        <div className="save-bar">
          <span>
            <strong>{count(picked.n, "file")}</strong> selected · {bytes(picked.size)}
          </span>
          <button
            type="button"
            className="ghost small"
            onClick={() => setWhole(allSelected ? new Set() : new Set(groups.map((g) => g.id)))}
          >
            {allSelected ? "Clear" : "Select all"}
          </button>
          <button
            type="button"
            className="primary"
            disabled={picked.n === 0}
            onClick={() =>
              onTransfer({
                groups: [...whole],
                files: [...files.keys()].filter((p) => ![...whole].some((g) => p.startsWith(`${g}/`))),
              })
            }
          >
            Transfer selected
          </button>
        </div>
      )}

      {browsing && (
        <FilePicker
          folder={clone.folder}
          group={browsing}
          whole={whole.has(browsing.id)}
          picked={files}
          onChange={setFiles}
          onWhole={(value) => {
            const next = new Set(whole);
            if (value) next.add(browsing.id);
            else next.delete(browsing.id);
            setWhole(next);
          }}
          onClose={() => setBrowsing(null)}
        />
      )}
    </div>
  );
}

const PAGE = 120;

function FilePicker({
  folder,
  group,
  whole,
  picked,
  onChange,
  onWhole,
  onClose,
}: {
  folder: string;
  group: CloneGroup;
  whole: boolean;
  picked: Map<string, number>;
  onChange: (next: Map<string, number>) => void;
  onWhole: (value: boolean) => void;
  onClose: () => void;
}) {
  const [items, setItems] = useState<CloneItem[]>([]);
  const [total, setTotal] = useState(0);

  useEffect(() => {
    cloneItems(folder, group.id, 0, PAGE)
      .then(([page, n]) => {
        setItems(page);
        setTotal(n);
      })
      .catch(() => setItems([]));
  }, [folder, group.id]);

  useEffect(() => {
    const key = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [onClose]);

  const more = () =>
    void cloneItems(folder, group.id, items.length, PAGE).then(([page]) => setItems([...items, ...page]));

  const toggle = (item: CloneItem) => {
    if (whole) {
      // Turning one off inside a whole group: switch to picking files,
      // keeping every other file in the group, loaded on screen or not.
      void cloneItems(folder, group.id, 0, total).then(([all]) => {
        const next = new Map(picked);
        for (const it of all) if (it.path !== item.path) next.set(it.path, it.size);
        onWhole(false);
        onChange(next);
      });
      return;
    }
    const next = new Map(picked);
    if (next.has(item.path)) next.delete(item.path);
    else next.set(item.path, item.size);
    onChange(next);
  };

  const inGroup = [...picked.keys()].filter((p) => p.startsWith(`${group.id}/`)).length;

  return (
    <div className="overlay" onClick={onClose} role="presentation">
      <div className="detail picker" role="dialog" aria-modal aria-label={group.label} onClick={(e) => e.stopPropagation()}>
        <header className="detail-head">
          <h2 className="detail-title">{group.id === "Photos & Videos" ? "Photos and videos" : group.label}</h2>
          <button type="button" className="ghost small" onClick={onClose}>
            Done
          </button>
        </header>
        <div className="row picker-bar">
          <span className="muted small-text">
            {whole ? `All ${total.toLocaleString()} included` : `${inGroup.toLocaleString()} of ${total.toLocaleString()} chosen`}
          </span>
          <button type="button" className="ghost small" onClick={() => onWhole(!whole)}>
            {whole ? "Include none" : "Include all"}
          </button>
        </div>
        <div className="pick-grid">
          {items.map((it) => {
            const on = whole || picked.has(it.path);
            const visual = it.media === "image" || it.media === "video";
            return (
              <button
                key={it.path}
                type="button"
                className={`pick ${on ? "on" : ""}`}
                onClick={() => toggle(it)}
                aria-pressed={on}
                title={`${it.name} · ${when(it.time, false)}`}
              >
                {visual ? (
                  <CloneThumb folder={folder} path={it.path} className="pick-thumb" />
                ) : (
                  <div className="pick-thumb">
                    <Icon name={MEDIA_ICON[it.media]} size={22} stroke={1.4} />
                  </div>
                )}
                <span className="pick-name">{it.name}</span>
                <span className="pick-mark" aria-hidden>
                  {on && <Icon name="check" size={12} stroke={3} />}
                </span>
              </button>
            );
          })}
        </div>
        {items.length < total && (
          <button type="button" className="ghost more" onClick={more}>
            Show more ({(total - items.length).toLocaleString()} left)
          </button>
        )}
      </div>
    </div>
  );
}
