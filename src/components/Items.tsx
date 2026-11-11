import { useEffect, useRef, useState } from "react";

import { MEDIA_GLYPH, SOURCE_LABEL, bytes, when } from "../lib/format";
import { reveal, thumbnail, type Finding } from "../lib/native";

const thumbs = new Map<number, string | null>();

/** Loads a thumbnail once the card scrolls into view, and remembers it. */
function useThumbnail(finding: Finding) {
  const ref = useRef<HTMLDivElement>(null);
  const [url, setUrl] = useState<string | null>(thumbs.get(finding.id) ?? null);
  const wants = finding.path !== null && ["image", "video", "document"].includes(finding.media);

  useEffect(() => {
    if (!wants || thumbs.has(finding.id) || !ref.current) return;
    const el = ref.current;
    const seen = new IntersectionObserver(([entry]) => {
      if (!entry?.isIntersecting) return;
      seen.disconnect();
      thumbnail(finding.id)
        .then((u) => {
          thumbs.set(finding.id, u);
          setUrl(u);
        })
        .catch(() => thumbs.set(finding.id, null));
    });
    seen.observe(el);
    return () => seen.disconnect();
  }, [finding.id, wants]);

  return { ref, url };
}

export function FindingCard({ finding: f, onOpen }: { finding: Finding; onOpen: () => void }) {
  const { ref, url } = useThumbnail(f);
  return (
    <button type="button" className={`card ${f.status}`} onClick={onOpen}>
      <div className="thumb" ref={ref}>
        {url ? (
          <img src={url} alt="" />
        ) : f.text ? (
          <p className="snippet">{f.text}</p>
        ) : (
          <span className="thumb-glyph" aria-hidden>
            {MEDIA_GLYPH[f.media]}
          </span>
        )}
        <span className={`badge ${f.status}`}>
          {f.status === "corrupt" ? "Damaged" : SOURCE_LABEL[f.source]}
        </span>
      </div>
      <div className="card-text">
        <span className="card-name">{f.name}</span>
        <span className="card-meta">{when(f.time, f.timeEstimated)}</span>
      </div>
    </button>
  );
}

export function Detail({
  finding: f,
  appName,
  onClose,
}: {
  finding: Finding;
  appName: string | null;
  onClose: () => void;
}) {
  const { ref, url } = useThumbnail(f);

  useEffect(() => {
    const key = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [onClose]);

  return (
    <div className="overlay" onClick={onClose} role="presentation">
      <div
        className="detail"
        role="dialog"
        aria-modal
        aria-label={f.name}
        onClick={(e) => e.stopPropagation()}
      >
        <header className="detail-head">
          <h2 className="selectable">{f.name}</h2>
          <button type="button" className="ghost small" onClick={onClose}>
            Close
          </button>
        </header>
        {(url || !f.text) && (
          <div className="detail-preview" ref={ref}>
            {url ? (
              <img src={url} alt="" />
            ) : (
              <span className="thumb-glyph" aria-hidden>
                {MEDIA_GLYPH[f.media]}
              </span>
            )}
          </div>
        )}
        {f.text && <pre className="recovered selectable">{f.text}</pre>}
        <dl className="facts">
          <dt>Why it is shown</dt>
          <dd className="selectable">{f.reason}</dd>
          <dt>When</dt>
          <dd>
            {when(f.time, f.timeEstimated)}
            {f.timeEstimated && f.time !== null && (
              <span className="muted"> (estimated from nearby data)</span>
            )}
          </dd>
          {f.app && (
            <>
              <dt>App</dt>
              <dd className="selectable">
                {appName ?? f.app}
                {appName && appName !== f.app && <span className="muted"> ({f.app})</span>}
              </dd>
            </>
          )}
          {f.originalPath && (
            <>
              <dt>On the device</dt>
              <dd className="selectable path">{f.originalPath}</dd>
            </>
          )}
          {f.size > 0 && (
            <>
              <dt>Size</dt>
              <dd>{bytes(f.size)}</dd>
            </>
          )}
        </dl>
        {f.path && (
          <button type="button" className="ghost" onClick={() => void reveal(f.path!)}>
            Show in Finder
          </button>
        )}
      </div>
    </div>
  );
}
