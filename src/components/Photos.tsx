import { useCallback, useEffect, useMemo, useState } from "react";

import { SOURCE_LABEL, bytes, count, when } from "../lib/format";
import {
  localOffsetMinutes,
  query,
  recover,
  reveal,
  type Device,
  type Exported,
  type Finding,
  type Location,
  type Media,
  type Query,
  type ScanSummary,
} from "../lib/native";
import { Icon } from "./Icon";
import { Thumb, Viewer } from "./Items";
import { SaveTo } from "./SaveTo";

/** The filters along the top. Each one is a question about where a photo was found. */
type Filter = "all" | "photosRecentlyDeleted" | "trashFolder" | "carved" | "recordOnly" | "damaged";

const FILTERS: { id: Filter; label: string; test: (f: Finding) => boolean }[] = [
  { id: "all", label: "All", test: () => true },
  { id: "photosRecentlyDeleted", label: "Photos Recently Deleted", test: (f) => f.source === "photosRecentlyDeleted" && f.path !== null },
  { id: "trashFolder", label: "Trash", test: (f) => f.source === "trashFolder" },
  { id: "carved", label: "Rebuilt from app data", test: (f) => f.source === "carvedFromDatabase" },
  { id: "recordOnly", label: "Only a record left", test: (f) => f.path === null },
  { id: "damaged", label: "Damaged", test: (f) => f.status === "corrupt" },
];

const ask = (media: Media): Query => ({
  text: "",
  app: null,
  status: null,
  media,
  zoom: "day",
  from: null,
  to: null,
  tzOffsetMinutes: localOffsetMinutes(),
  limit: 20000,
});

function dayKey(t: number | null): string {
  if (t === null) return "undated";
  const d = new Date(t * 1000);
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
}

function dayLabel(t: number | null): string {
  if (t === null) return "Date unknown";
  return new Date(t * 1000).toLocaleDateString(undefined, { weekday: "long", day: "numeric", month: "long", year: "numeric" });
}

const clock = (t: number | null) =>
  t === null ? "" : new Date(t * 1000).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });

type Saving = { kind: "idle" } | { kind: "saving" } | { kind: "saved"; result: Exported } | { kind: "failed"; message: string };

/**
 * Photos: every deleted photo (and, if wanted, video), by day. Tap the
 * circle to pick, open one to look closer, then recover what was picked.
 * Only what is picked is copied, so space is used only then.
 */
export function Photos({
  scanned,
  location,
  onLocation,
  onScanMac,
  onFiles,
}: {
  scanned: { device: Device; summary: ScanSummary } | null;
  location: Location | null;
  onLocation: (location: Location) => void;
  onScanMac: () => void;
  onFiles: () => void;
}) {
  const [images, setImages] = useState<Finding[] | null>(null);
  const [videos, setVideos] = useState<Finding[]>([]);
  const [withVideos, setWithVideos] = useState(true);
  const [filter, setFilter] = useState<Filter>("all");
  const [picked, setPicked] = useState<Set<number>>(new Set());
  const [open, setOpen] = useState<number | null>(null);
  const [saving, setSaving] = useState<Saving>({ kind: "idle" });

  useEffect(() => {
    setImages(null);
    setPicked(new Set());
    if (!scanned) return;
    let live = true;
    void Promise.all([query(ask("image")), query(ask("video"))])
      .then(([i, v]) => {
        if (!live) return;
        setImages(i.items);
        setVideos(v.items);
      })
      .catch(() => live && setImages([]));
    return () => {
      live = false;
    };
  }, [scanned]);

  const all = useMemo(() => {
    const list = [...(images ?? []), ...(withVideos ? videos : [])];
    return list.sort((a, b) => (b.time ?? -Infinity) - (a.time ?? -Infinity));
  }, [images, videos, withVideos]);

  const shown = useMemo(() => {
    const test = FILTERS.find((f) => f.id === filter)?.test ?? (() => true);
    return all.filter(test);
  }, [all, filter]);

  const days = useMemo(() => {
    const groups: { key: string; label: string; items: Finding[] }[] = [];
    for (const f of shown) {
      const key = dayKey(f.time);
      let g = groups.find((x) => x.key === key);
      if (!g) {
        g = { key, label: dayLabel(f.time), items: [] };
        groups.push(g);
      }
      g.items.push(f);
    }
    return groups;
  }, [shown]);

  const recoverable = (f: Finding) => f.path !== null;
  const toggle = useCallback((f: Finding) => {
    if (!recoverable(f)) return;
    setPicked((p) => {
      const next = new Set(p);
      if (next.has(f.id)) next.delete(f.id);
      else next.add(f.id);
      return next;
    });
  }, []);

  const pickedItems = all.filter((f) => picked.has(f.id));
  const pickedBytes = pickedItems.reduce((n, f) => n + f.size, 0);
  const nounFor = (items: Finding[]) => (items.every((f) => f.media === "image") ? "photo" : "item");

  const run = async (ids: number[]) => {
    setSaving({ kind: "saving" });
    try {
      const result = await recover(ids);
      setSaving({ kind: "saved", result });
      setPicked((p) => {
        const next = new Set(p);
        ids.forEach((id) => next.delete(id));
        return next;
      });
    } catch (cause) {
      setSaving({ kind: "failed", message: cause instanceof Error ? cause.message : String(cause) });
    }
  };

  if (!scanned) {
    return (
      <div className="home empty-home">
        <h1 className="home-title">No photos to show yet</h1>
        <p className="home-lede">
          Scan this Mac, a drive or a phone. Every deleted photo it finds shows up here, by day, ready
          to recover one at a time.
        </p>
        <button type="button" className="primary big" onClick={onScanMac}>
          <Icon name="search" size={18} stroke={2.2} />
          Scan this Mac
        </button>
      </div>
    );
  }
  if (images === null) {
    return <p className="muted pad-page">Gathering photos…</p>;
  }
  if (all.length === 0) {
    return (
      <div className="home empty-home">
        <h1 className="home-title">No deleted photos on {scanned.device.name}</h1>
        <p className="home-lede">
          {scanned.summary.findings > 0
            ? `It found ${count(scanned.summary.findings, "other deleted or damaged item")}, such as messages and documents.`
            : "Nothing deleted or damaged turned up."}
        </p>
        {scanned.summary.findings > 0 && (
          <button type="button" className="secondary big" onClick={onFiles}>
            See them in Files
          </button>
        )}
      </div>
    );
  }

  const openIndex = open === null ? -1 : shown.findIndex((f) => f.id === open);
  const current = openIndex >= 0 ? shown[openIndex] : null;

  return (
    <div className="photos-page">
      <div className="photos-head">
        <div>
          <h1 className="page-title">{count(all.length, withVideos && videos.length ? "deleted photo or video" : "deleted photo", withVideos && videos.length ? "deleted photos and videos" : "deleted photos")}</h1>
          <p className="muted">
            From {scanned.device.name}. Tap the circle to pick, or open one to look closer.
          </p>
        </div>
      </div>

      <div className="chips" role="toolbar" aria-label="Where they were found">
        {FILTERS.map((f) => {
          const n = all.filter(f.test).length;
          if (f.id !== "all" && n === 0) return null;
          return (
            <button
              key={f.id}
              type="button"
              className={`chip ${filter === f.id ? "on" : ""}`}
              aria-pressed={filter === f.id}
              onClick={() => setFilter(f.id)}
            >
              {f.label}
              <b>{n.toLocaleString()}</b>
            </button>
          );
        })}
        <span className="chips-gap" />
        {videos.length > 0 && (
          <button
            type="button"
            className={`chip ${withVideos ? "on" : ""}`}
            aria-pressed={withVideos}
            onClick={() => setWithVideos(!withVideos)}
          >
            Videos too<b>{videos.length.toLocaleString()}</b>
          </button>
        )}
      </div>

      {saving.kind === "saved" && (
        <p className="status-line">
          <span className="status-mark ok" />
          Recovered {count(saving.result.files, "item")} to {saving.result.folder}
          <button type="button" className="text-button" onClick={() => void reveal(saving.result.folder)}>
            Show in Finder
          </button>
        </p>
      )}
      {saving.kind === "failed" && (
        <p className="status-line selectable">
          <span className="status-mark bad" />
          {saving.message}
        </p>
      )}

      <div className="photo-days">
        {days.map((g) => {
          const pickable = g.items.filter(recoverable);
          const allPicked = pickable.length > 0 && pickable.every((f) => picked.has(f.id));
          return (
            <section key={g.key} className="photo-day">
              <div className="photo-day-head">
                <h2>
                  {g.label} <span className="muted">· {count(g.items.length, "item")}</span>
                </h2>
                {pickable.length > 0 && (
                  <button
                    type="button"
                    className="text-button small-text"
                    onClick={() =>
                      setPicked((p) => {
                        const next = new Set(p);
                        pickable.forEach((f) => (allPicked ? next.delete(f.id) : next.add(f.id)));
                        return next;
                      })
                    }
                  >
                    {allPicked ? "Unpick all" : "Pick all"}
                  </button>
                )}
              </div>
              <div className="photo-grid">
                {g.items.map((f) => (
                  <div key={f.id} className={`photo-tile ${picked.has(f.id) ? "on" : ""}`}>
                    <button type="button" className="photo-open" onClick={() => setOpen(f.id)} aria-label={`Open ${f.name}`}>
                      <Thumb finding={f} className="photo-thumb" />
                      <span className="photo-tag">
                        {f.status === "corrupt" ? (
                          <span className="bad-text">damaged</span>
                        ) : f.path === null ? (
                          <span className="warn-text">record only</span>
                        ) : (
                          clock(f.time)
                        )}
                      </span>
                      {f.media === "video" && (
                        <span className="photo-kind">
                          <Icon name="video" size={12} />
                        </span>
                      )}
                    </button>
                    {recoverable(f) && (
                      <button
                        type="button"
                        className="photo-tick"
                        aria-pressed={picked.has(f.id)}
                        aria-label={picked.has(f.id) ? `Unpick ${f.name}` : `Pick ${f.name}`}
                        onClick={() => toggle(f)}
                      />
                    )}
                  </div>
                ))}
              </div>
            </section>
          );
        })}
      </div>

      {picked.size > 0 && (
        <footer className="pick-bar">
          <strong>{picked.size.toLocaleString()} picked</strong>
          <span className="muted">{bytes(pickedBytes)}</span>
          <SaveTo location={location} onChange={onLocation} compact />
          <span className="chips-gap" />
          <button type="button" className="text-button" onClick={() => setPicked(new Set())}>
            Clear
          </button>
          <button
            type="button"
            className="primary"
            disabled={saving.kind === "saving" || location?.available === false}
            onClick={() => void run([...picked])}
          >
            {saving.kind === "saving" ? "Recovering…" : `Recover ${count(picked.size, nounFor(pickedItems))}`}
          </button>
        </footer>
      )}

      {current && (
        <PhotoView
          finding={current}
          index={openIndex}
          total={shown.length}
          picked={picked.has(current.id)}
          pickedCount={picked.size}
          location={location}
          saving={saving.kind === "saving"}
          onPick={() => toggle(current)}
          onRecover={() => void run([current.id])}
          onMove={(step) => {
            const next = shown[openIndex + step];
            if (next) setOpen(next.id);
          }}
          onClose={() => setOpen(null)}
        />
      )}
    </div>
  );
}

/** One photo, large, with where it was found and a button to recover just it. */
function PhotoView({
  finding: f,
  index,
  total,
  picked,
  pickedCount,
  location,
  saving,
  onPick,
  onRecover,
  onMove,
  onClose,
}: {
  finding: Finding;
  index: number;
  total: number;
  picked: boolean;
  pickedCount: number;
  location: Location | null;
  saving: boolean;
  onPick: () => void;
  onRecover: () => void;
  onMove: (step: number) => void;
  onClose: () => void;
}) {
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
      else if (e.key === "ArrowRight") onMove(1);
      else if (e.key === "ArrowLeft") onMove(-1);
      else if (e.key === " ") {
        e.preventDefault();
        onPick();
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [onClose, onMove, onPick]);

  const has = f.path !== null;
  const health =
    f.status === "corrupt"
      ? { mark: "bad", text: "Damaged: parts of it may be missing" }
      : has
        ? { mark: "ok", text: "The whole file is still there" }
        : { mark: "warn", text: "Only its record is left; the file itself is gone" };

  return (
    <div className="photo-view" role="dialog" aria-modal aria-label={f.name}>
      <div className="photo-stage">
        <button type="button" className="text-button back" onClick={onClose}>
          ‹ All photos
        </button>
        <div className="photo-frame">
          <Viewer finding={f} />
          {index > 0 && (
            <button type="button" className="nav-arrow left" aria-label="Previous" onClick={() => onMove(-1)}>
              ‹
            </button>
          )}
          {index < total - 1 && (
            <button type="button" className="nav-arrow right" aria-label="Next" onClick={() => onMove(1)}>
              ›
            </button>
          )}
        </div>
        <p className="muted small-text">
          {index + 1} of {total.toLocaleString()} · ← → to move, space to pick
        </p>
      </div>
      <aside className="photo-facts">
        <div>
          <h2 className="selectable photo-name">{f.name}</h2>
          <p className="muted">{when(f.time, f.timeEstimated)}</p>
        </div>
        <span className="status-line">
          <span className={`status-mark ${health.mark}`} />
          {health.text}
        </span>
        <dl className="fact-rows">
          <div>
            <dt>Found in</dt>
            <dd>{SOURCE_LABEL[f.source]}</dd>
          </div>
          {f.originalPath && (
            <div>
              <dt>Was at</dt>
              <dd className="selectable path">{f.originalPath}</dd>
            </div>
          )}
          {f.size > 0 && (
            <div>
              <dt>Size</dt>
              <dd>{bytes(f.size)}</dd>
            </div>
          )}
          <div>
            <dt>Why it is shown</dt>
            <dd>{f.reason}</dd>
          </div>
        </dl>
        <div className="photo-actions">
          <button type="button" className="primary wide-button" disabled={!has || saving} onClick={onRecover}>
            {saving ? "Recovering…" : f.media === "video" ? "Recover this video" : "Recover this photo"}
          </button>
          {has && (
            <button type="button" className="secondary wide-button" onClick={onPick}>
              {picked ? "Remove from picked" : `Add to picked${pickedCount ? ` (${pickedCount})` : ""}`}
            </button>
          )}
          <p className="muted small-text center-text">
            Saves a copy to {location?.label ?? "the Save to location"}. The original is left where it was.
          </p>
        </div>
      </aside>
    </div>
  );
}
