import { useCallback, useEffect, useRef, useState } from "react";

import { CloneDone, CloneSheet, RestoreDone, RestoreSheet } from "./components/Clone";
import { Results } from "./components/Results";
import { Backups } from "./components/Backups";
import { ScanPanel, type Stage } from "./components/ScanPanel";
import { TopBar, type Section } from "./components/TopBar";
import { AwaitingPhone, TransferPrompt, TransferView, albumFor } from "./components/Transfer";
import { bytes, count } from "./lib/format";
import {
  cancelScan,
  cloneDevice,
  cloneGroups,
  discover,
  listClones,
  nativeAvailable,
  onProgress,
  restoreClone,
  scan,
  transferClone,
  type CloneEntry,
  type CloneOptions,
  type Device,
  type Discovery,
  type ScanSummary,
  type Selection,
} from "./lib/native";

const message = (cause: unknown) => (cause instanceof Error ? cause.message : String(cause));
const isPhone = (d: Device) => d.kind === "iphone" || d.kind === "android";

/** The newest clone that can go onto `device`: from another phone, with something to move. */
function cloneFor(device: Device, clones: CloneEntry[]): CloneEntry | null {
  return (
    clones.find(
      (c) =>
        c.info.deviceId !== device.id &&
        (device.kind === "android" || (c.info.kind === "iphone" && (c.backupPresent || c.zipPresent))),
    ) ?? null
  );
}

/**
 * Pick a device on the left, press Scan, and see only what was deleted or
 * damaged on it. Or clone a phone, and move the clone onto a new one.
 *
 * Colour carries state and nothing else: the window is black while idle,
 * amber while Banker works, red once deleted items are on screen, green
 * when something has been saved. The `data-state` attribute on the shell
 * drives it (see styles.css).
 */
export function App() {
  const [discovery, setDiscovery] = useState<Discovery>({ devices: [], hints: [] });
  const [looking, setLooking] = useState(false);
  const [selected, setSelected] = useState<Device | null>(null);
  const [stage, setStage] = useState<Stage>({ kind: "idle" });
  const [scanned, setScanned] = useState<{ device: Device; summary: ScanSummary } | null>(null);
  const [sheet, setSheet] = useState<{ kind: "clone" | "restore"; device: Device } | null>(null);
  const [offer, setOffer] = useState<{ device: Device; clone: CloneEntry } | null>(null);
  const [view, setView] = useState<"device" | "backups">("device");
  const seenPhones = useRef<Set<string> | null>(null);
  const declined = useRef<Set<string>>(new Set());
  const stageRef = useRef(stage);
  stageRef.current = stage;

  /** A phone was just plugged in: offer to move a clone of another phone onto it. */
  const phoneArrived = useCallback(async (device: Device) => {
    if (declined.current.has(device.id) || device.problem) return;
    const now = stageRef.current;
    const clones = await listClones().catch(() => [] as CloneEntry[]);
    const clone = now.kind === "awaiting" ? now.clone : cloneFor(device, clones);
    if (!clone || clone.info.deviceId === device.id) return;
    setSelected(device);
    if (now.kind === "awaiting") {
      setStage({ kind: "choosing", device, clone });
    } else if (now.kind === "idle" || now.kind === "cloned" || now.kind === "failed") {
      setOffer({ device, clone });
    }
  }, []);

  const refresh = useCallback(async () => {
    if (!nativeAvailable()) return;
    setLooking(true);
    try {
      const found = await discover();
      setDiscovery(found);
      // A phone counts as arriving when it is plugged in while Banker is
      // open, or becomes ready (it was waiting for Allow or Trust).
      const ready = found.devices.filter((d) => isPhone(d) && !d.problem);
      const before = seenPhones.current;
      seenPhones.current = new Set(ready.map((d) => d.id));
      for (const d of ready) {
        if (before && !before.has(d.id)) void phoneArrived(d);
      }
    } catch (cause) {
      setStage({ kind: "failed", message: message(cause) });
    } finally {
      setLooking(false);
    }
  }, [phoneArrived]);

  useEffect(() => {
    void refresh();
    // Devices come and go; look again every few seconds while visible.
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible") void refresh();
    }, 5000);
    return () => window.clearInterval(timer);
  }, [refresh]);

  useEffect(() => {
    const stop = onProgress((progress) =>
      setStage((s) =>
        s.kind === "scanning" || s.kind === "cloning" || s.kind === "restoring" || s.kind === "transferring"
          ? { ...s, progress }
          : s,
      ),
    );
    return () => {
      void stop.then((f) => f());
    };
  }, []);

  const start = async (device: Device) => {
    setSelected(device);
    setView("device");
    setOffer(null);
    setStage({ kind: "scanning", device, progress: null });
    try {
      const summary = await scan(device);
      setScanned({ device, summary });
      setStage({ kind: "idle" });
    } catch (cause) {
      setStage({ kind: "failed", message: message(cause) });
    }
  };

  const startClone = async (device: Device, options: CloneOptions) => {
    setSheet(null);
    setStage({ kind: "cloning", device, progress: null, zip: options.makeZip });
    try {
      const clone = await cloneDevice(device, options);
      setStage({ kind: "cloned", device, clone });
    } catch (cause) {
      setStage({ kind: "failed", message: message(cause) });
    }
  };

  const startRestore = async (device: Device, folder: string, password: string | null) => {
    setSheet(null);
    setOffer(null);
    setStage({ kind: "restoring", device, progress: null });
    try {
      setStage({ kind: "restored", device, message: await restoreClone(folder, device, password) });
    } catch (cause) {
      setStage({ kind: "failed", message: message(cause) });
    }
  };

  const startTransfer = async (device: Device, clone: CloneEntry, selection: Selection) => {
    setOffer(null);
    setStage({ kind: "transferring", device, progress: null });
    try {
      const result = await transferClone(clone.folder, device, selection, albumFor(clone));
      setStage({ kind: "transferred", device, result });
    } catch (cause) {
      setStage({ kind: "failed", message: message(cause) });
    }
  };

  /** Scan a finished clone for deleted items, as if it were a backup. */
  const scanClone = (path: string, name: string) =>
    void start({ id: `clone:${path}`, kind: "backup", name: `${name} (clone)`, detail: path, path, problem: null });

  /** "Transfer to another phone": use one that is already connected, or wait for it. */
  const transferFrom = (clone: CloneEntry) => {
    setView("device");
    const other = discovery.devices.find((d) => isPhone(d) && d.id !== clone.info.deviceId && !d.problem);
    if (other) {
      setSelected(other);
      setStage({ kind: "choosing", device: other, clone });
    } else {
      setStage({ kind: "awaiting", clone });
    }
  };

  const phoneReady = selected !== null && isPhone(selected) && !selected.problem;

  /** A section in the top bar was chosen. */
  const openSection = async (section: Section) => {
    if (section === "backups") {
      setView("backups");
      return;
    }
    setView("device");
    if (section === "scan") {
      if (stage.kind !== "failed") setStage({ kind: "idle" });
    } else if ((section === "clone" || section === "restore") && selected && phoneReady) {
      setSheet({ kind: section, device: selected });
    } else if (section === "transfer") {
      const clones = await listClones().catch(() => [] as CloneEntry[]);
      const clone = selected && phoneReady ? cloneFor(selected, clones) : (clones[0] ?? null);
      if (!clone) {
        setStage({
          kind: "failed",
          message: "There is no clone of another phone on this Mac yet. Clone the old phone first.",
        });
      } else if (selected && phoneReady && clone.info.deviceId !== selected.id) {
        setStage({ kind: "choosing", device: selected, clone });
      } else {
        setStage({ kind: "awaiting", clone });
      }
    }
  };

  const working =
    stage.kind === "scanning" ||
    stage.kind === "cloning" ||
    stage.kind === "restoring" ||
    stage.kind === "transferring";
  const done = stage.kind === "cloned" || stage.kind === "restored" || stage.kind === "transferred";
  const away = stage.kind === "awaiting" || stage.kind === "choosing";
  const showResults =
    view === "device" && !working && !done && !away && scanned !== null && scanned.device.id === selected?.id;
  const active: Section =
    view === "backups"
      ? "backups"
      : stage.kind === "cloning" || stage.kind === "cloned"
        ? "clone"
        : stage.kind === "restoring" || stage.kind === "restored"
          ? "restore"
          : away || stage.kind === "transferring" || stage.kind === "transferred"
            ? "transfer"
            : "scan";
  const state = working
    ? "working"
    : done
      ? "saved"
      : showResults && scanned.summary.findings > 0
        ? "found"
        : "idle";

  const body = () => {
    if (view === "backups" && !working) {
      return (
        <Backups
          finderBackups={discovery.devices.filter((d) => d.kind === "backup")}
          onScan={(d) => void start(d)}
          onTransfer={(clone) => transferFrom(clone)}
        />
      );
    }
    switch (stage.kind) {
      case "cloned":
        return (
          <CloneDone
            clone={stage.clone}
            onScan={(path) => scanClone(path, stage.device.name)}
            onTransfer={() => transferFrom(stage.clone)}
            onBack={() => setStage({ kind: "idle" })}
          />
        );
      case "restored":
        return <RestoreDone message={stage.message} onBack={() => setStage({ kind: "idle" })} />;
      case "transferred":
        return (
          <RestoreDone
            title={`Moved to ${stage.device.name}`}
            message={`${count(stage.result.files, "file")} (${bytes(stage.result.bytes)}) are on the phone. ${stage.result.notes.join(" ")}`}
            onBack={() => setStage({ kind: "idle" })}
          />
        );
      case "awaiting":
        return <AwaitingPhone clone={stage.clone} onCancel={() => setStage({ kind: "idle" })} />;
      case "choosing":
        return (
          <TransferView
            device={stage.device}
            clone={stage.clone}
            onTransfer={(selection) => void startTransfer(stage.device, stage.clone, selection)}
            onRestore={() => setSheet({ kind: "restore", device: stage.device })}
            onBack={() => setStage({ kind: "idle" })}
          />
        );
      default:
        return showResults ? (
          <Results device={scanned.device} summary={scanned.summary} onRescan={() => void start(scanned.device)} />
        ) : (
          <ScanPanel
            device={selected}
            stage={stage}
            onScan={(d) => void start(d)}
            onClone={(d) => setSheet({ kind: "clone", device: d })}
            onRestore={(d) => setSheet({ kind: "restore", device: d })}
            onCancel={() => void cancelScan()}
          />
        );
    }
  };

  return (
    <div className="shell" data-state={state}>
      <TopBar
        discovery={discovery}
        looking={looking}
        selected={selected}
        active={active}
        working={
          working
            ? {
                id: stage.device.id,
                label:
                  stage.kind === "cloning"
                    ? "cloning"
                    : stage.kind === "restoring"
                      ? "restoring"
                      : stage.kind === "transferring"
                        ? "receiving files"
                        : "scanning",
              }
            : null
        }
        foundId={scanned && scanned.summary.findings > 0 ? scanned.device.id : null}
        disabled={working}
        unavailable={new Set<Section>(phoneReady ? [] : ["clone", "restore"])}
        onSection={(section) => void openSection(section)}
        onSelect={(d) => {
          setSelected(d);
          setView("device");
          if (stage.kind === "failed" || done || away) setStage({ kind: "idle" });
        }}
        onRefresh={() => void refresh()}
      />
      <main className="main">
        {!nativeAvailable() && (
          <p className="banner">
            Running in a browser tab. Start the app with <code>npm run dev</code> to reach devices.
          </p>
        )}
        {body()}
        {offer && !working && (
          <TransferPrompt
            device={offer.device}
            clone={offer.clone}
            onChoose={() => {
              setStage({ kind: "choosing", device: offer.device, clone: offer.clone });
              setOffer(null);
            }}
            onAll={async () => {
              const groups = await cloneGroups(offer.clone.folder).catch(() => []);
              void startTransfer(offer.device, offer.clone, { groups: groups.map((g) => g.id), files: [] });
            }}
            onDismiss={() => {
              declined.current.add(offer.device.id);
              setOffer(null);
            }}
          />
        )}
        {sheet?.kind === "clone" && (
          <CloneSheet
            device={sheet.device}
            onStart={(options) => void startClone(sheet.device, options)}
            onClose={() => setSheet(null)}
          />
        )}
        {sheet?.kind === "restore" && (
          <RestoreSheet
            device={sheet.device}
            onStart={(clone, password) => void startRestore(sheet.device, clone.folder, password)}
            onClose={() => setSheet(null)}
          />
        )}
      </main>
    </div>
  );
}
