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
import { Icon } from "./Icon";
import { Detail, FindingCard } from "./Items";
import { Overview, type Pick } from "./Overview";
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

type Save = { kind: "idle" } | { kind: "saving" } | { kind: "saved"; result: Exported };

/** Results: an overview of what was found, then the timeline behind it. */
export function Results({ device, summary, onRescan }: Props) {
  const [view, setView] = useState<"overview" | "timeline">("overview");
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
  const [save, setSave] = useState<Save>({ kind: "idle" });

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
    if (view !== "timeline") return;
    let live = true;
    query(q)
      .then((r) => live && (setResult(r), setError(null)))
      .catch((e) => live && setError(String(e)));
    return () => {
      live = false;
    };
  }, [q, view]);

  useEffect(() => setLimit(PAGE), [debounced, app, status, media, here]);

  const saveMatching = async (which: Query) => {
    setSave({ kind: "saving" });
    try {
      setSave({ kind: "saved", result: await exportMatching(which) });
    } catch (e) {
      setError(String(e));
      setSave({ kind: "idle" });
    }
  };

  const openPick = (pick: Pick) => {
    setStatus(pick.status);
    setMedia(pick.media);
    setApp(pick.app ?? null);
    setText("");
    setCrumbs([ALL_TIME]);
    setView("timeline");
  };

  const chip = <T,>(value: T | null, current: T | null, set: (v: T | null) => void, label: string, dot?: string) => (
    <button
      key={label}
      type="button"
      className={`chip ${current === value ? "on" : ""}`}
      onClick={() => set(value)}
    >
      {dot && <span className={`dot ${dot}`} />}
      {label}
    </button>
  );

  const saveBanner = save.kind === "saved" && (
    <p className="banner saved">
      <Icon name="check" size={15} stroke={2.4} />
      Saved {count(save.result.files, "file")} and {count(save.result.records, "record")} to{" "}
      <button type="button" className="link" onClick={() => void reveal(save.result.folder)}>
        {save.result.folder}
      </button>
      {save.result.failed > 0 && ` (${save.result.failed} could not be copied)`}
    </p>
  );

  if (view === "overview") {
    return (
      <Overview
        device={device}
        summary={summary}
        saving={save.kind === "saving"}
        banner={saveBanner || null}
        onRescan={onRescan}
        onPick={openPick}
        onSaveAll={() =>
          void saveMatching({ ...ALL_QUERY, tzOffsetMinutes: localOffsetMinutes() })
        }
      />
    );
  }

  return (
    <div className="results">
      <header className="topbar">
        <button type="button" className="back" onClick={() => setView("overview")}>
          <Icon name="back" size={16} stroke={2} />
          Overview
        </button>
        <span className="muted">{device.name} · Timeline</span>
        <button
          type="button"
          className={save.kind === "saving" ? "working-button" : "primary"}
          onClick={() => void saveMatching({ ...q, limit: PAGE })}
          disabled={save.kind === "saving" || !result?.total}
        >
          {save.kind === "saving" ? "Saving…" : "Save these to Mac"}
        </button>
      </header>

      {saveBanner}
      {error && <p className="banner error selectable">{error}</p>}

      <div className="filters">
        <label className="search">
          <Icon name="search" size={16} stroke={2} />
          <input
            type="search"
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder="Search names, apps and recovered text"
            aria-label="Search"
          />
        </label>
        <div className="segmented" role="group" aria-label="Show">
          {chip<Status>(null, status, setStatus, "Everything")}
          {chip<Status>("deleted", status, setStatus, "Deleted", "deleted")}
          {chip<Status>("corrupt", status, setStatus, "Damaged", "damaged")}
        </div>
      </div>
      <div className="chips">
        {chip<Media>(null, media, setMedia, "All kinds")}
        {MEDIA_FILTERS.map((m) => chip<Media>(m, media, setMedia, MEDIA_LABEL[m]))}
      </div>

      {result && result.apps.length > 0 && (
        <div className="apps" role="group" aria-label="Apps">
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
        onZoom={(zoom) => setCrumbs([...crumbs.slice(0, -1), { ...here, zoom }])}
        onPick={(b) =>
          setCrumbs([...crumbs, { label: b.label, from: b.start, to: b.end, zoom: finer(here.zoom) }])
        }
      />

      {result && (
        <>
          <p className="muted small-text list-head">
            {count(result.total, "item")}
            {result.deleted > 0 && (
              <>
                {" · "}
                <span className="deleted-text">{result.deleted.toLocaleString()} deleted</span>
              </>
            )}
            {result.corrupt > 0 && (
              <>
                {" · "}
                <span className="damaged-text">{result.corrupt.toLocaleString()} damaged</span>
              </>
            )}
            {result.undated > 0 &&
              here.from === null &&
              ` · ${result.undated.toLocaleString()} without a date, shown last`}
          </p>
          {result.total === 0 ? (
            <div className="empty">Nothing deleted or damaged matches.</div>
          ) : (
            <div className="grid">
              {result.items.map((f) => (
                <FindingCard
                  key={f.id}
                  finding={f}
                  selected={open?.id === f.id}
                  onOpen={() => setOpen(f)}
                />
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

const ALL_QUERY: Query = {
  text: "",
  app: null,
  status: null,
  media: null,
  zoom: "year",
  from: null,
  to: null,
  tzOffsetMinutes: 0,
  limit: PAGE,
};
