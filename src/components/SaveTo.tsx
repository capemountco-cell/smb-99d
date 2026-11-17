import { bytes } from "../lib/format";
import { chooseFolder, setLocation, type Location } from "../lib/native";
import { Icon } from "./Icon";

/**
 * Where Reco saves scan copies, clones and saved items, with the free
 * space there and a way to change it (an external drive, for example).
 */
export function SaveTo({
  location,
  onChange,
  compact = false,
}: {
  location: Location | null;
  onChange: (location: Location) => void;
  compact?: boolean;
}) {
  if (!location) return null;

  const change = async () => {
    const folder = await chooseFolder();
    if (folder) onChange(await setLocation(folder));
  };
  const reset = async () => onChange(await setLocation(null));

  return (
    <div className={`save-to ${location.available ? "" : "missing"} ${compact ? "compact" : ""}`}>
      <span className="save-to-icon" aria-hidden>
        <Icon name={location.label === "This Mac" ? "folder" : "drive"} size={16} />
      </span>
      <span className="save-to-text">
        <span className="save-to-label">
          Save to <strong>{location.label}</strong>
          {location.available && location.freeBytes !== null && (
            <span className="muted"> · {bytes(location.freeBytes)} free</span>
          )}
        </span>
        {!location.available ? (
          <span className="save-to-path">Not connected. Plug in the drive or choose another location.</span>
        ) : (
          !compact && <span className="save-to-path selectable">{location.path}</span>
        )}
      </span>
      <button type="button" className="secondary small" onClick={() => void change()}>
        Change…
      </button>
      {!location.isDefault && !compact && (
        <button type="button" className="text-button small-text" onClick={() => void reset()}>
          Use this Mac
        </button>
      )}
    </div>
  );
}
