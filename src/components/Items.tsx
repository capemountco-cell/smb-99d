import { useEffect, useRef, useState } from "react";

import { MEDIA_ICON, SOURCE_LABEL, bytes, when } from "../lib/format";
import { reveal, thumbnail, type Finding } from "../lib/native";
import { Icon } from "./Icon";

const thumbs = new Map<number, string | null>();

/** Loads a thumbnail once the element scrolls into view, and remembers it. */
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

/** A picture of the finding, or its kind's icon when there is none. */
export function Thumb({ finding: f, className }: { finding: Finding; className: string }) {
  const { ref, url } = useThumbnail(f);
  return (
    <div className={`${className} ${f.status}`} ref={ref}>
      {url ? <img src={url} alt="" /> : <Icon name={MEDIA_ICON[f.media]} size={26} stroke={1.5} />}
    </div>
  );
}

export function FindingCard({
  finding: f,
  selected,
  onOpen,
}: {
  finding: Finding;
  selected: boolean;
  onOpen: () => void;
}) {
  const { ref, url } = useThumbnail(f);
  return (
    <button type="button" className={`card ${f.status} ${selected ? "selected" : ""}`} onClick={onOpen}>
      <div className="thumb" ref={ref}>
        {url ? (
          <img src={url} alt="" />
        ) : f.text ? (
          <p className="snippet">{f.text}</p>
        ) : (
          <span className="thumb-icon">
            <Icon name={MEDIA_ICON[f.media]} size={30} stroke={1.4} />
          </span>
        )}
        <span className={`tag ${f.status === "corrupt" ? "damaged" : "deleted"} on-thumb`}>
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
  const damaged = f.status === "corrupt";

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
          <span className={`tag ${damaged ? "damaged" : "deleted"}`}>
            {damaged ? "Damaged" : SOURCE_LABEL[f.source]}
          </span>
          <button type="button" className="ghost small" onClick={onClose}>
            Close
          </button>
        </header>
        <h2 className="selectable detail-title">{f.name}</h2>
        {(url || !f.text) && (
          <div className={`detail-preview ${f.status}`} ref={ref}>
            {url ? <img src={url} alt="" /> : <Icon name={MEDIA_ICON[f.media]} size={44} stroke={1.3} />}
          </div>
        )}
        {f.text && <pre className="recovered selectable">{f.text}</pre>}
        <dl className="facts">
          <dt>Why it is shown</dt>
          <dd className="selectable">{f.reason}</dd>
          <dt>When</dt>
          <dd>
            {when(f.time, f.timeEstimated)}
            {f.timeEstimated && f.time !== null && <span className="muted"> (estimated)</span>}
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
          <button type="button" className="ghost detail-action" onClick={() => void reveal(f.path!)}>
            Show in Finder
          </button>
        )}
      </div>
    </div>
  );
}
