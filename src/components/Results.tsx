import { useEffect, useMemo, useState } from "react";

import { MEDIA_LABEL, count, finer } from "../lib/format";
import {
  exportMatching,
  localOffsetMinutes,
  query,
  reveal,
  type Device,
  type Exported,
  type Finding,
  type Media,
  type Query,
  type QueryResult,
  type ScanSummary,
  type Status,
  type Zoom,
} from "../lib/native";
import { Detail, FindingCard } from "./Items";
import { Timeline } from "./Timeline";

interface Crumb {
  label: string;
  from: number | null;
  to: number | null;
  zoom: Zoom;
}

const ALL_TIME: Crumb = { label: "All time", from: null, to: null, zoom: "year" };
const PAGE = 300;
const MEDIA_FILTERS: Media[] = ["image", "video", "text", "document", "audio", "other"];

interface Props {
  device: Device;
  summary: ScanSummary;
  onRescan: () => void;
}

export function Results({ device, summary, onRescan }: Props) {
  const [text, setText] = useState("");
  const [debounced, setDebounced] = useState("");
  const [status, setStatus] = useState<Status | null>(null);
  const [media, setMedia] = useState<Media | null>(null);
  const [app, setApp] = useState<string | null>(null);
  const [crumbs, setCrumbs] = useState<Crumb[]>([ALL_TIME]);
  const [limit, setLimit] = useState(PAGE);
  const [result, setResult] = useState<QueryResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [open, setOpen] = useState<Finding | null>(null);
  const [saved, setSaved] = useState<Exported | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    const t = window.setTimeout(() => setDebounced(text), 200);
    return () => window.clearTimeout(t);
  }, [text]);

  const here = crumbs[crumbs.length - 1] ?? ALL_TIME;
  const q: Query = useMemo(
    () => ({
      text: debounced,
      app,
      status,
      media,
      zoom: here.zoom,
      from: here.from,
      to: here.to,
      tzOffsetMinutes: localOffsetMinutes(),
      limit,
    }),
    [debounced, app, status, media, here, limit],
  );

  useEffect(() => {
    let live = true;
    query(q)
      .then((r) => live && (setResult(r), setError(null)))
      .catch((e) => live && setError(String(e)));
    return () => {
      live = false;
    };
  }, [q]);

  useEffect(() => setLimit(PAGE), [debounced, app, status, media, here]);

  const save = async () => {
    setSaving(true);
    try {
      setSaved(await exportMatching(q));
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  const chip = <T,>(value: T | null, current: T | null, set: (v: T | null) => void, label: string) => (
    <button
      key={label}
      type="button"
      className={`chip ${current === value ? "on" : ""}`}
      onClick={() => set(value)}
    >
      {label}
    </button>
  );

  return (
    <div className="results">
      <header className="results-head">
        <div>
          <h2>{device.name}</h2>
          <p className="muted small-text">
            {count(summary.filesChecked, "file")} and {count(summary.databasesChecked, "app database")}{" "}
            checked
            {result && (
              <>
                {" · "}
                <strong>{count(result.deleted, "deleted item")}</strong>
                {" · "}
                <strong>{count(result.corrupt, "damaged item")}</strong> shown
              </>
            )}
          </p>
        </div>
        <div className="row">
          <button type="button" className="ghost" onClick={onRescan}>
            Scan again
          </button>
          <button type="button" onClick={() => void save()} disabled={saving || !result?.total}>
            {saving ? "Saving…" : "Save these to Mac"}
          </button>
        </div>
      </header>

      {summary.notes.map((n) => (
        <p key={n} className="banner selectable">
          {n}
        </p>
      ))}
      {saved && (
        <p className="banner ok">
          Saved {count(saved.files, "file")} and {count(saved.records, "record")} to{" "}
          <button type="button" className="link" onClick={() => void reveal(saved.folder)}>
            {saved.folder}
          </button>
          {saved.failed > 0 && ` (${saved.failed} could not be copied)`}
        </p>
      )}
      {error && <p className="banner error selectable">{error}</p>}

      <div className="filters">
        <input
          className="search"
          type="search"
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder="Search names, apps and recovered text"
          aria-label="Search"
        />
        <div className="chips">
          {chip<Status>(null, status, setStatus, "Everything")}
          {chip<Status>("deleted", status, setStatus, "Deleted")}
          {chip<Status>("corrupt", status, setStatus, "Damaged")}
        </div>
        <div className="chips">
          {chip<Media>(null, media, setMedia, "All kinds")}
          {MEDIA_FILTERS.map((m) => chip<Media>(m, media, setMedia, MEDIA_LABEL[m]))}
        </div>
      </div>

      {result && result.apps.length > 0 && (
        <div className="apps" role="list" aria-label="Apps">
          <button
            type="button"
            className={`app-pill ${app === null ? "on" : ""}`}
            onClick={() => setApp(null)}
          >
            All apps
          </button>
          {result.apps.map((a) => (
            <button
              key={a.id}
              type="button"
              className={`app-pill ${app === a.id ? "on" : ""}`}
              onClick={() => setApp(app === a.id ? null : a.id)}
              title={a.id}
            >
              <span className="app-initial" aria-hidden>
                {a.name.slice(0, 1).toUpperCase()}
              </span>
              {a.name}
              <span className="pill-count">{a.count.toLocaleString()}</span>
            </button>
          ))}
        </div>
      )}

      <Timeline
        crumbs={crumbs}
        buckets={result?.buckets ?? []}
        onCrumb={(i) => setCrumbs(crumbs.slice(0, i + 1))}
        onZoom={(zoom) =>
          setCrumbs([...crumbs.slice(0, -1), { ...here, zoom }])
        }
        onPick={(b) =>
          setCrumbs([...crumbs, { label: b.label, from: b.start, to: b.end, zoom: finer(here.zoom) }])
        }
      />

      {result && (
        <>
          <p className="muted small-text list-head">
            {count(result.total, "item")}
            {result.undated > 0 && here.from === null && ` · ${result.undated.toLocaleString()} without a date, shown last`}
          </p>
          {result.total === 0 ? (
            <div className="empty">
              <p>Nothing deleted or damaged matches.</p>
            </div>
          ) : (
            <div className="grid">
              {result.items.map((f) => (
                <FindingCard key={f.id} finding={f} onOpen={() => setOpen(f)} />
              ))}
            </div>
          )}
          {result.items.length < result.total && (
            <button type="button" className="ghost more" onClick={() => setLimit(limit + PAGE)}>
              Show more ({(result.total - result.items.length).toLocaleString()} left)
            </button>
          )}
        </>
      )}

      {open && (
        <Detail
          finding={open}
          appName={result?.apps.find((a) => a.id === open.app)?.name ?? null}
          onClose={() => setOpen(null)}
        />
      )}
    </div>
  );
}
