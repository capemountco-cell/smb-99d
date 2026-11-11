import { ZOOMS, daysInMonth, localParts, monthTitle } from "../lib/format";
import type { Bucket, Zoom } from "../lib/native";

interface Crumb {
  label: string;
  zoom: Zoom;
}

interface Props {
  crumbs: Crumb[];
  buckets: Bucket[];
  onCrumb: (index: number) => void;
  onZoom: (zoom: Zoom) => void;
  onPick: (bucket: Bucket) => void;
}

const ZOOM_LABEL: Record<Zoom, string> = {
  year: "Years",
  month: "Months",
  day: "Days",
  hour: "Hours",
  minute: "Minutes",
};

/**
 * The timeline: bars for years, months, hours and minutes; a calendar for
 * days. Clicking a bar or a day zooms into it.
 */
export function Timeline({ crumbs, buckets, onCrumb, onZoom, onPick }: Props) {
  const zoom = crumbs[crumbs.length - 1]?.zoom ?? "year";
  const max = Math.max(1, ...buckets.map((b) => b.count));
  const leaf = zoom === "minute" && crumbs.length > 1;

  return (
    <section className="timeline" aria-label="Timeline">
      <div className="timeline-head">
        <nav className="crumbs" aria-label="Where you are on the timeline">
          {crumbs.map((c, i) => (
            <span key={`${c.label}-${i}`}>
              {i > 0 && <span className="sep">›</span>}
              <button
                type="button"
                className={`crumb ${i === crumbs.length - 1 ? "current" : ""}`}
                onClick={() => onCrumb(i)}
              >
                {c.label}
              </button>
            </span>
          ))}
        </nav>
        <div className="segmented" role="group" aria-label="Group by">
          {ZOOMS.map((z) => (
            <button
              key={z}
              type="button"
              className={z === zoom ? "on" : ""}
              onClick={() => onZoom(z)}
            >
              {ZOOM_LABEL[z]}
            </button>
          ))}
        </div>
      </div>

      {buckets.length === 0 ? (
        <p className="muted small-text">Nothing with a date here.</p>
      ) : zoom === "day" ? (
        <Calendar buckets={buckets} max={max} onPick={onPick} />
      ) : (
        <div className="bars">
          {buckets.map((b) => (
            <button
              key={b.start}
              type="button"
              className="bar"
              disabled={leaf}
              onClick={() => onPick(b)}
              title={`${b.label}: ${b.count.toLocaleString()} (${b.images.toLocaleString()} photos)`}
            >
              <span className="bar-count">{b.count.toLocaleString()}</span>
              <span className="bar-fill" style={{ height: `${8 + (b.count / max) * 92}%` }}>
                {b.images > 0 && (
                  <span className="bar-images" style={{ height: `${(b.images / b.count) * 100}%` }} />
                )}
              </span>
              <span className="bar-label">{b.label}</span>
            </button>
          ))}
        </div>
      )}
    </section>
  );
}

function Calendar({
  buckets,
  max,
  onPick,
}: {
  buckets: Bucket[];
  max: number;
  onPick: (b: Bucket) => void;
}) {
  const byMonth = new Map<string, { year: number; month: number; days: Map<number, Bucket> }>();
  for (const b of buckets) {
    // The middle of the bucket avoids daylight-saving edge cases.
    const p = localParts((b.start + b.end) / 2);
    const key = `${p.year}-${p.month}`;
    if (!byMonth.has(key)) byMonth.set(key, { year: p.year, month: p.month, days: new Map() });
    byMonth.get(key)!.days.set(p.day, b);
  }
  const weekdays = ["S", "M", "T", "W", "T", "F", "S"];

  return (
    <div className="months">
      {[...byMonth.values()].map(({ year, month, days }) => {
        const lead = new Date(year, month, 1).getDay();
        const total = daysInMonth(year, month);
        return (
          <div key={`${year}-${month}`} className="month">
            <h3>{monthTitle(year, month)}</h3>
            <div className="cal">
              {weekdays.map((w, i) => (
                <span key={`h${i}`} className="cal-head">
                  {w}
                </span>
              ))}
              {Array.from({ length: lead }, (_, i) => (
                <span key={`b${i}`} />
              ))}
              {Array.from({ length: total }, (_, i) => {
                const day = i + 1;
                const b = days.get(day);
                if (!b) {
                  return (
                    <span key={day} className="cal-day empty-day">
                      {day}
                    </span>
                  );
                }
                const heat = 0.18 + (b.count / max) * 0.82;
                return (
                  <button
                    key={day}
                    type="button"
                    className="cal-day hit"
                    style={{ ["--heat" as string]: heat }}
                    onClick={() => onPick(b)}
                    title={`${b.label}: ${b.count.toLocaleString()}`}
                  >
                    <span>{day}</span>
                    <span className="cal-count">{b.count.toLocaleString()}</span>
                  </button>
                );
              })}
            </div>
          </div>
        );
      })}
    </div>
  );
}
