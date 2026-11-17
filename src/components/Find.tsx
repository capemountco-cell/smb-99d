import { useCallback, useEffect, useRef, useState } from "react";

import { bytes, MEDIA_ICON } from "../lib/format";
import {
  findFiles,
  findThumbnail,
  getFindSettings,
  hideFind,
  onFindOpen,
  openMain,
  openPath,
  recover,
  recoverPaths,
  reveal,
  setFindDeleted,
  setFindExcludes,
  setFindShortcut,
  setStartsAtLogin,
  shortcutSymbols,
  type FindHit,
  type FindPrefs,
  type Found,
} from "../lib/native";
import { Icon } from "./Icon";

const EXAMPLES = [
  "pdfs from gmail last week",
  "photo of a receipt",
  "videos over 1 GB",
  "photos from Greece",
  "invoice for $2,000",
  "deleted this month",
];

const message = (cause: unknown) => (cause instanceof Error ? cause.message : String(cause));

const thumbs = new Map<string, string | null>();

/** A small picture of an image or video result, fetched when it is shown. */
function HitThumb({ hit, big = false }: { hit: FindHit; big?: boolean }) {
  const pictured = hit.media === "image" || hit.media === "video" || hit.media === "document";
  const [src, setSrc] = useState<string | null>(thumbs.get(hit.path) ?? null);
  useEffect(() => {
    if (!pictured || thumbs.has(hit.path)) {
      setSrc(thumbs.get(hit.path) ?? null);
      return;
    }
    let live = true;
    findThumbnail(hit.path)
      .then((s) => {
        thumbs.set(hit.path, s);
        if (live) setSrc(s);
      })
      .catch(() => thumbs.set(hit.path, null));
    return () => {
      live = false;
    };
  }, [hit.path, pictured]);
  const ext = hit.name.includes(".") ? hit.name.split(".").pop()!.slice(0, 4).toUpperCase() : "";
  return (
    <span className={`hit-thumb ${big ? "big" : ""}`}>
      {src ? (
        <img src={src} alt="" />
      ) : big ? (
        <Icon name={MEDIA_ICON[hit.media]} size={36} stroke={1.5} />
      ) : (
        <span className="hit-ext">{ext || <Icon name={MEDIA_ICON[hit.media]} size={14} />}</span>
      )}
    </span>
  );
}

const whenText = (t: number | null) =>
  t === null
    ? ""
    : new Date(t * 1000).toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" });

/**
 * Find: type what you remember, see what Reco understood as chips you can
 * drop, then the files. Deleted ones are marked and can be recovered.
 * `panel` is the floating window the shortcut opens; otherwise it sits
 * inside the Find section.
 */
export function FindPanel({ panel, shortcut }: { panel: boolean; shortcut: string }) {
  const [query, setQuery] = useState("");
  const [drop, setDrop] = useState<string[]>([]);
  const [found, setFound] = useState<Found | null>(null);
  const [busy, setBusy] = useState(false);
  const [at, setAt] = useState(0);
  const [note, setNote] = useState<{ ok: boolean; text: string; folder?: string } | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const asked = useRef(0);

  const run = useCallback(async (q: string, d: string[]) => {
    const id = ++asked.current;
    if (!q.trim()) {
      setFound(null);
      setBusy(false);
      return;
    }
    setBusy(true);
    try {
      const f = await findFiles(q, d);
      if (id === asked.current) {
        setFound(f);
        setAt(0);
      }
    } catch (cause) {
      if (id === asked.current) setNote({ ok: false, text: message(cause) });
    } finally {
      if (id === asked.current) setBusy(false);
    }
  }, []);

  useEffect(() => {
    const t = window.setTimeout(() => void run(query, drop), 220);
    return () => window.clearTimeout(t);
  }, [query, drop, run]);

  // Each time the shortcut opens the panel: a fresh, focused box.
  useEffect(() => {
    if (!panel) return;
    const stop = onFindOpen(() => {
      setNote(null);
      input.current?.focus();
      input.current?.select();
    });
    input.current?.focus();
    return () => {
      void stop.then((f) => f());
    };
  }, [panel]);

  const hits = found?.hits ?? [];
  const current = hits[at] ?? null;

  const openHit = (h: FindHit) => {
    if (h.deleted) return void recoverHit(h);
    void openPath(h.path).then(() => panel && void hideFind());
  };
  const recoverHit = async (h: FindHit) => {
    try {
      const folder = h.deleted?.finding != null ? (await recover([h.deleted.finding])).folder : await recoverPaths([h.path]);
      setNote({ ok: true, text: `Recovered ${h.name}`, folder });
    } catch (cause) {
      setNote({ ok: false, text: message(cause) });
    }
  };

  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape" && panel) {
        e.preventDefault();
        void hideFind();
      } else if (e.key === "ArrowDown") {
        e.preventDefault();
        setAt((a) => Math.min(a + 1, Math.max(hits.length - 1, 0)));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setAt((a) => Math.max(a - 1, 0));
      } else if (e.key === "Enter" && current) {
        e.preventDefault();
        if (e.metaKey) void reveal(current.path);
        else openHit(current);
      } else if (e.key.toLowerCase() === "r" && e.metaKey && current?.deleted) {
        e.preventDefault();
        void recoverHit(current);
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  });

  return (
    <section className={`find ${panel ? "panel" : "inline"}`} aria-label="Find">
      <div className="find-bar">
        <Icon name="search" size={20} stroke={2} />
        <label htmlFor="find-q" className="sr-only">
          Find a file
        </label>
        <input
          id="find-q"
          ref={input}
          className="find-input"
          value={query}
          autoFocus
          autoComplete="off"
          spellCheck={false}
          placeholder="Describe the file you remember"
          onChange={(e) => {
            setQuery(e.target.value);
            setDrop([]);
          }}
        />
        {busy ? <span className="faint small-text">Looking…</span> : found && <span className="muted small-text">{hits.length} found</span>}
        {panel && <kbd>esc</kbd>}
      </div>

      {found && found.parts.length > 0 && (
        <div className="find-parts">
          <span className="faint small-text">Understood as</span>
          {found.parts.map((p) => (
            <button
              key={p.key}
              type="button"
              className="find-chip"
              aria-label={`Remove ${p.label} ${p.value}`}
              onClick={() => setDrop([...drop, p.key])}
            >
              {p.label} <b>{p.value}</b> <span aria-hidden>×</span>
            </button>
          ))}
        </div>
      )}

      {note && (
        <p className="status-line find-note">
          <span className={`status-mark ${note.ok ? "ok" : "bad"}`} />
          {note.text}
          {note.folder && (
            <button type="button" className="text-button" onClick={() => void reveal(note.folder!)}>
              Show in Finder
            </button>
          )}
        </p>
      )}

      {!query.trim() ? (
        <div className="find-empty">
          <span className="section-label">Try</span>
          <div className="find-examples">
            {EXAMPLES.map((e) => (
              <button key={e} type="button" className="find-example" onClick={() => setQuery(e)}>
                {e}
              </button>
            ))}
          </div>
          <p className="muted small-text">
            Press <b>{shortcutSymbols(shortcut)}</b> in any app to open Find. ⌘Space stays Spotlight.
          </p>
        </div>
      ) : found && hits.length === 0 && !busy ? (
        <div className="find-empty">
          <p className="muted">
            Nothing matches.{found.parts.length > 1 ? " Remove a chip above to widen it." : ""}
            {!found.spotlight && " Spotlight did not answer; check it is turned on for this Mac."}
          </p>
        </div>
      ) : (
        <div className="find-body">
          <ul className="find-list" role="listbox" aria-label="Results">
            {hits.map((h, i) => (
              <li key={`${h.path}:${i}`} role="option" aria-selected={i === at}>
                <button
                  type="button"
                  className={`find-row ${i === at ? "on" : ""}`}
                  onMouseEnter={() => setAt(i)}
                  onClick={() => setAt(i)}
                  onDoubleClick={() => openHit(h)}
                >
                  <HitThumb hit={h} />
                  <span className="find-row-text">
                    <span className="find-name">{h.name}</span>
                    <span className="find-where">
                      {h.deleted ? (
                        <>
                          <span className="status-mark warn" />
                          {h.deleted.label}
                        </>
                      ) : (
                        <>
                          {h.place}
                          {h.cameFrom && ` · from ${h.cameFrom}`}
                        </>
                      )}
                    </span>
                  </span>
                  <span className="find-meta">
                    {whenText(h.time)}
                    {h.size > 0 && ` · ${bytes(h.size)}`}
                  </span>
                </button>
              </li>
            ))}
          </ul>
          {current && (
            <aside className="find-preview">
              <HitThumb hit={current} big />
              <h2 className="selectable">{current.name}</h2>
              <dl className="fact-rows">
                {current.deleted ? (
                  <div>
                    <dt>Deleted</dt>
                    <dd>{current.deleted.label}</dd>
                  </div>
                ) : (
                  <div>
                    <dt>Where</dt>
                    <dd>{current.place}</dd>
                  </div>
                )}
                {current.cameFrom && (
                  <div>
                    <dt>Came from</dt>
                    <dd>{current.cameFrom}</dd>
                  </div>
                )}
                {current.time !== null && (
                  <div>
                    <dt>{current.deleted ? "When" : "Changed"}</dt>
                    <dd>{new Date(current.time * 1000).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" })}</dd>
                  </div>
                )}
                <div>
                  <dt>Size</dt>
                  <dd>{bytes(current.size)}</dd>
                </div>
              </dl>
              <div className="find-actions">
                {current.deleted ? (
                  <button type="button" className="primary" onClick={() => void recoverHit(current)}>
                    Recover
                  </button>
                ) : (
                  <button type="button" className="primary" onClick={() => openHit(current)}>
                    Open
                  </button>
                )}
                <button type="button" className="secondary" onClick={() => void reveal(current.path)}>
                  Show in Finder
                </button>
              </div>
            </aside>
          )}
        </div>
      )}

      <footer className="find-foot">
        <span>Everything stays on this Mac</span>
        <span className="find-keys">
          <span>↑↓ move</span>
          <span>↵ open</span>
          <span>⌘↵ show in Finder</span>
          <span>⌘R recover</span>
          {panel && (
            <button type="button" className="text-button small-text" onClick={() => void openMain()}>
              Open Reco
            </button>
          )}
        </span>
      </footer>
    </section>
  );
}

/** The Find window the shortcut opens: only the panel. */
export function FindWindow() {
  const [shortcut, setShortcut] = useState("Alt+Space");
  useEffect(() => {
    getFindSettings()
      .then((p) => setShortcut(p.shortcut))
      .catch(() => {});
  }, []);
  return (
    <div className="find-window">
      <FindPanel panel shortcut={shortcut} />
    </div>
  );
}

const MOD_ORDER = ["Ctrl", "Alt", "Shift", "Cmd"] as const;

/** Turn a key press into "Ctrl+Alt+Space", or null while only modifiers are held. */
function recorded(e: KeyboardEvent): string | null {
  if (["Meta", "Control", "Alt", "Shift"].includes(e.key)) return null;
  const mods = MOD_ORDER.filter((m) =>
    m === "Ctrl" ? e.ctrlKey : m === "Alt" ? e.altKey : m === "Shift" ? e.shiftKey : e.metaKey,
  );
  const code = e.code;
  const key = code === "Space" ? "Space" : code.startsWith("Key") ? code.slice(3) : code.startsWith("Digit") ? code.slice(5) : code;
  return [...mods, key].join("+");
}

/** The Find section: the shortcut and settings, with Find right there too. */
export function FindSection() {
  const [prefs, setPrefs] = useState<FindPrefs | null>(null);
  const [recording, setRecording] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [folder, setFolder] = useState("");

  useEffect(() => {
    getFindSettings()
      .then(setPrefs)
      .catch((e) => setProblem(message(e)));
  }, []);

  useEffect(() => {
    if (!recording) return;
    const key = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") return setRecording(false);
      const s = recorded(e);
      if (!s) return;
      setRecording(false);
      setFindShortcut(s)
        .then((p) => {
          setPrefs(p);
          setProblem(null);
        })
        .catch((err) => setProblem(message(err)));
    };
    window.addEventListener("keydown", key, true);
    return () => window.removeEventListener("keydown", key, true);
  }, [recording]);

  const change = (p: Promise<FindPrefs>) =>
    void p.then(setPrefs).catch((e) => setProblem(message(e)));

  return (
    <div className="find-page">
      <div className="find-page-head">
        <div>
          <h1 className="page-title">Find any file</h1>
          <p className="muted">
            Describe it the way you remember it. Spotlight answers for names, dates, where a download came
            from and the text inside documents; Reco adds what was deleted.
          </p>
        </div>
      </div>
      {!recording && <FindPanel panel={false} shortcut={prefs?.shortcut ?? "Alt+Space"} />}

      <div className="find-settings">
        <section className="ov-card">
          <h2>Open Find from anywhere</h2>
          <div className="shortcut-row">
            {(prefs?.shortcut ?? "Alt+Space").split("+").map((k) => (
              <kbd key={k} className="key-cap">
                {shortcutSymbols(k)}
              </kbd>
            ))}
            <span className="chips-gap" />
            <button type="button" className="secondary" onClick={() => setRecording(!recording)}>
              {recording ? "Press the new shortcut… (esc to cancel)" : "Record new…"}
            </button>
          </div>
          {problem ? (
            <p className="status-line">
              <span className="status-mark bad" />
              {problem}
            </p>
          ) : prefs?.warning ? (
            <p className="status-line">
              <span className="status-mark warn" />
              {prefs.warning}
            </p>
          ) : (
            prefs && (
              <p className="status-line">
                <span className="status-mark ok" />
                {shortcutSymbols(prefs.shortcut)} is free. ⌘Space stays Spotlight.
              </p>
            )
          )}
          <p className="faint small-text">
            Reco never takes ⌘Space (Spotlight), ⌘⌥Space (Finder search) or ⌃Space (keyboard language).
          </p>
        </section>

        <section className="ov-card">
          <h2>Also</h2>
          <label className="check-row">
            <input
              type="checkbox"
              checked={prefs?.startsAtLogin ?? false}
              onChange={(e) => change(setStartsAtLogin(e.target.checked))}
            />
            Open Reco when I log in, so the shortcut always works
          </label>
          <label className="check-row">
            <input
              type="checkbox"
              checked={prefs?.includeDeleted ?? true}
              onChange={(e) => change(setFindDeleted(e.target.checked))}
            />
            Include deleted files in results
          </label>
          <p className="faint small-text">
            Closing the window keeps Reco in the menu bar, so the shortcut keeps working. Quit it from there.
          </p>
        </section>

        <section className="ov-card">
          <h2>Leave out</h2>
          <div className="exclude-list">
            {(prefs?.excludes ?? []).map((x) => (
              <button
                key={x}
                type="button"
                className="find-chip"
                aria-label={`Stop leaving out ${x}`}
                onClick={() => prefs && change(setFindExcludes(prefs.excludes.filter((e) => e !== x)))}
              >
                {x} <span aria-hidden>×</span>
              </button>
            ))}
          </div>
          <form
            className="row"
            onSubmit={(e) => {
              e.preventDefault();
              const f = folder.trim();
              if (!f || !prefs) return;
              change(setFindExcludes([...prefs.excludes.filter((x) => x !== f), f]));
              setFolder("");
            }}
          >
            <label htmlFor="exclude" className="sr-only">
              Folder to leave out
            </label>
            <input id="exclude" value={folder} onChange={(e) => setFolder(e.target.value)} placeholder="~/Private or a folder name" />
            <button type="submit" className="secondary small">
              Add
            </button>
          </form>
        </section>
      </div>
    </div>
  );
}
