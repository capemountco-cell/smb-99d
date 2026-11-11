import { useEffect, useRef, useState } from "react";

import { MEDIA_ICON, SOURCE_LABEL, bytes, when } from "../lib/format";
import {
  cardPreview,
  fileUrl,
  preview,
  reveal,
  type CardPreview,
  type Finding,
  type Preview,
} from "../lib/native";
import { Icon } from "./Icon";

const cards = new Map<number, CardPreview>();

/**
 * A card's preview (a small picture or the opening lines), loaded once the
 * card scrolls into view and remembered. Recovered records already carry
 * their text, so they need no round trip.
 */
function useCardPreview(f: Finding) {
  const ref = useRef<HTMLDivElement>(null);
  const known = f.text ? { image: null, text: f.text } : cards.get(f.id);
  const [shown, setShown] = useState<CardPreview | null>(known ?? null);
  const wants = !known && f.path !== null;

  useEffect(() => {
    if (!wants || !ref.current) return;
    const el = ref.current;
    const seen = new IntersectionObserver(([entry]) => {
      if (!entry?.isIntersecting) return;
      seen.disconnect();
      cardPreview(f.id)
        .then((p) => {
          cards.set(f.id, p);
          setShown(p);
        })
        .catch(() => cards.set(f.id, { image: null, text: null }));
    });
    seen.observe(el);
    return () => seen.disconnect();
  }, [f.id, wants]);

  return { ref, shown };
}

/** A small square picture of a finding, for the overview mosaic. */
export function Thumb({ finding: f, className }: { finding: Finding; className: string }) {
  const { ref, shown } = useCardPreview(f);
  return (
    <div className={`${className} ${f.status}`} ref={ref}>
      {shown?.image ? (
        <img src={shown.image} alt="" />
      ) : (
        <Icon name={MEDIA_ICON[f.media]} size={26} stroke={1.5} />
      )}
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
  const { ref, shown } = useCardPreview(f);
  return (
    <button type="button" className={`card ${f.status} ${selected ? "selected" : ""}`} onClick={onOpen}>
      <div className="thumb" ref={ref}>
        {shown?.image ? (
          <img src={shown.image} alt="" />
        ) : shown?.text ? (
          <p className="snippet">{shown.text}</p>
        ) : (
          <span className="thumb-icon">
            <Icon name={MEDIA_ICON[f.media]} size={30} stroke={1.4} />
          </span>
        )}
        {f.media === "video" && shown?.image && (
          <span className="play-mark" aria-hidden>
            <Icon name="play" size={16} stroke={2} />
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

/** The item itself: the picture, a player, the document, or its text. */
function Viewer({ finding: f }: { finding: Finding }) {
  const [shown, setShown] = useState<Preview | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let live = true;
    setShown(null);
    setFailed(false);
    preview(f.id)
      .then((p) => live && setShown(p))
      .catch((e) => live && setShown({ kind: "nothing", reason: String(e) }));
    return () => {
      live = false;
    };
  }, [f.id]);

  if (!shown) {
    return (
      <div className={`viewer ${f.status} loading`}>
        <Icon name={MEDIA_ICON[f.media]} size={40} stroke={1.3} />
      </div>
    );
  }

  const cannot = (what: string) => (
    <div className={`viewer ${f.status}`}>
      <Icon name={MEDIA_ICON[f.media]} size={40} stroke={1.3} />
      <p className="muted small-text">
        This Mac cannot {what} this file
        {f.status === "corrupt" ? ", probably because of the damage" : ""}. Save it to the Mac to
        try another app.
      </p>
    </div>
  );

  switch (shown.kind) {
    case "image":
      return failed ? (
        cannot("show")
      ) : (
        <div className={`viewer ${f.status} media`}>
          <img src={fileUrl(shown.path)} alt={f.name} onError={() => setFailed(true)} />
        </div>
      );
    case "picture":
      return (
        <div className={`viewer ${f.status} media`}>
          <img src={shown.dataUrl} alt={f.name} />
        </div>
      );
    case "video":
      return failed ? (
        cannot("play")
      ) : (
        <div className={`viewer ${f.status} media`}>
          <video src={fileUrl(shown.path)} controls preload="metadata" onError={() => setFailed(true)} />
        </div>
      );
    case "audio":
      return failed ? (
        cannot("play")
      ) : (
        <div className={`viewer ${f.status} audio`}>
          <Icon name="audio" size={40} stroke={1.3} />
          <audio src={fileUrl(shown.path)} controls preload="metadata" onError={() => setFailed(true)} />
        </div>
      );
    case "pdf":
      return (
        <div className={`viewer ${f.status} document`}>
          <iframe src={fileUrl(shown.path)} title={f.name} />
        </div>
      );
    case "text":
      return (
        <div className="text-view">
          <pre className={`recovered ${f.status} selectable`}>{shown.text}</pre>
          {shown.truncated && (
            <p className="muted small-text">Showing the first 64 KB. Save it to the Mac to read it all.</p>
          )}
        </div>
      );
    case "nothing":
      return (
        <div className={`viewer ${f.status}`}>
          <Icon name={MEDIA_ICON[f.media]} size={40} stroke={1.3} />
          <p className="muted small-text">{shown.reason}</p>
        </div>
      );
  }
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
        <Viewer finding={f} />
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
