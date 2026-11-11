import { useEffect, useState, type ReactNode } from "react";

import { count } from "../lib/format";
import {
  localOffsetMinutes,
  query,
  type Device,
  type Finding,
  type Media,
  type Query,
  type QueryResult,
  type ScanSummary,
  type Status,
} from "../lib/native";
import { Icon, type IconName } from "./Icon";
import { Thumb } from "./Items";

/** What a card opens on the timeline. */
export interface Pick {
  status: Status | null;
  media: Media | null;
  app?: string;
}

interface Props {
  device: Device;
  summary: ScanSummary;
  saving: boolean;
  banner: ReactNode;
  onRescan: () => void;
  onPick: (pick: Pick) => void;
  onSaveAll: () => void;
}

interface Numbers {
  deleted: QueryResult;
  corrupt: QueryResult;
  photos: QueryResult;
  messages: QueryResult;
}

const base = (over: Partial<Query>): Query => ({
  text: "",
  app: null,
  status: null,
  media: null,
  zoom: "year",
  from: null,
  to: null,
  tzOffsetMinutes: localOffsetMinutes(),
  limit: 1,
  ...over,
});

export function Overview({ device, summary, saving, banner, onRescan, onPick, onSaveAll }: Props) {
  const [n, setN] = useState<Numbers | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([
      query(base({ status: "deleted" })),
      query(base({ status: "corrupt" })),
      query(base({ status: "deleted", media: "image", limit: 6 })),
      query(base({ status: "deleted", media: "text" })),
    ])
      .then(([deleted, corrupt, photos, messages]) => setN({ deleted, corrupt, photos, messages }))
      .catch((e) => setError(String(e)));
  }, [summary]);

  if (error) return <p className="banner error selectable">{error}</p>;
  if (!n) return null;

  const m = n.deleted.byMedia;
  const pictures = (m.image ?? 0) + (m.video ?? 0);
  const messages = m.text ?? 0;
  const other = n.deleted.total - pictures - messages;
  const damaged = n.corrupt.total;
  const topApps = n.messages.apps.slice(0, 4);
  const widest = Math.max(1, ...topApps.map((a) => a.count));

  return (
    <div className="overview">
      <header className="topbar">
        <button type="button" className="back" onClick={onRescan}>
          <Icon name="back" size={16} stroke={2} />
          Scan again
        </button>
        <span className="muted">
          {device.name} · {count(summary.filesChecked, "file")} checked
        </span>
        <span className="topbar-spacer" />
      </header>

      <div className="headline">
        {n.deleted.total > 0 ? (
          <h1>
            Banker found <span className="deleted-text">{count(n.deleted.total, "deleted item")}</span>{" "}
            on {device.name}
          </h1>
        ) : (
          <h1>Nothing deleted was found on {device.name}</h1>
        )}
        <p className="lede">
          {damaged > 0 ? (
            <>
              And <span className="damaged-text">{count(damaged, "damaged file")}</span>.{" "}
            </>
          ) : null}
          Nothing on the device was changed. Open a group to see every item and when it went.
        </p>
        <button type="button" className="ghost" onClick={() => onPick({ status: null, media: null })}>
          Open the timeline
        </button>
      </div>

      {summary.notes.map((note) => (
        <p key={note} className="banner selectable">
          {note}
        </p>
      ))}
      {banner}

      <div className="groups">
        <Group
          className="tall"
          title={pictures > 0 ? `${count(pictures, "deleted photo or video", "deleted photos and videos")}` : "No deleted photos or videos"}
          body="From Recently Deleted, trash folders and space apps freed."
          tag={pictures > 0 ? "deleted" : null}
          meta={count(pictures, "item")}
          primary
          onReview={() => onPick({ status: "deleted", media: "image" })}
          disabled={pictures === 0}
        >
          {n.photos.items.length > 0 && (
            <div className="mosaic">
              {n.photos.items.map((f: Finding) => (
                <Thumb key={f.id} finding={f} className="mosaic-tile" />
              ))}
            </div>
          )}
        </Group>

        <Group
          className="wide"
          icon="message"
          title={messages > 0 ? count(messages, "deleted message or record", "deleted messages and records") : "No deleted messages"}
          body="Still inside each app's data. Pick an app to see only what it lost."
          tag={messages > 0 ? "deleted" : null}
          meta={count(n.messages.apps.length, "app")}
          onReview={() => onPick({ status: "deleted", media: "text" })}
          disabled={messages === 0}
        >
          {topApps.length > 0 && (
            <ul className="app-bars">
              {topApps.map((a) => (
                <li key={a.id}>
                  <button type="button" onClick={() => onPick({ status: "deleted", media: "text", app: a.id })}>
                    <span className="app-bar-name">{a.name}</span>
                    <span className="app-bar-track">
                      <span className="app-bar-fill" style={{ width: `${(a.count / widest) * 100}%` }} />
                    </span>
                    <span className="app-bar-count">{a.count.toLocaleString()}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </Group>

        <Group
          icon="trash"
          title={other > 0 ? count(other, "other deleted file") : "No other deleted files"}
          body="Documents, audio and anything else thrown away."
          tag={other > 0 ? "deleted" : null}
          meta=""
          onReview={() => onPick({ status: "deleted", media: null })}
          disabled={other === 0}
        />

        <Group
          className={damaged > 0 ? "damaged" : ""}
          icon="damaged"
          title={damaged > 0 ? count(damaged, "damaged file") : "No damaged files"}
          body="Cut short, zero-filled or failing their checks. They won't open as they are."
          tag={damaged > 0 ? "damaged" : null}
          meta=""
          onReview={() => onPick({ status: "corrupt", media: null })}
          disabled={damaged === 0}
        />
      </div>

      {summary.findings > 0 && (
        <div className="save-bar">
          <span>
            <strong>{count(summary.findings, "item")}</strong> can be saved to this Mac, sorted by app
            and day
          </span>
          <button
            type="button"
            className={saving ? "working-button" : "primary"}
            onClick={onSaveAll}
            disabled={saving}
          >
            {saving ? "Saving…" : "Save to Mac"}
          </button>
        </div>
      )}
    </div>
  );
}

function Group(props: {
  className?: string;
  icon?: IconName;
  title: string;
  body: string;
  tag: "deleted" | "damaged" | null;
  meta: string;
  primary?: boolean;
  disabled?: boolean;
  onReview: () => void;
  children?: ReactNode;
}) {
  const { className = "", icon, title, body, tag, meta, primary, disabled, onReview, children } = props;
  return (
    <section className={`group ${className} ${disabled ? "quiet" : ""}`}>
      <div className="group-head">
        <div>
          <h2>{title}</h2>
          <p className="muted">{body}</p>
        </div>
        {icon && (
          <span className="tile">
            <Icon name={icon} size={20} />
          </span>
        )}
      </div>
      {children}
      <div className="group-foot">
        <span className="group-meta">
          {tag && <span className={`tag ${tag}`}>{tag === "deleted" ? "Deleted" : "Damaged"}</span>}
          {meta}
        </span>
        <button type="button" className={primary ? "primary" : "ghost"} onClick={onReview} disabled={disabled}>
          Review
        </button>
      </div>
    </section>
  );
}
