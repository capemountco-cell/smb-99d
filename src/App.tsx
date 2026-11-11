import { useCallback, useEffect, useState } from "react";

import { CloneDone, CloneSheet, RestoreDone, RestoreSheet } from "./components/Clone";
import { Results } from "./components/Results";
import { ScanPanel, type Stage } from "./components/ScanPanel";
import { Sidebar } from "./components/Sidebar";
import {
  cancelScan,
  cloneDevice,
  discover,
  restoreClone,
  nativeAvailable,
  onProgress,
  scan,
  type Device,
  type CloneOptions,
  type Discovery,
  type ScanSummary,
} from "./lib/native";

const message = (cause: unknown) => (cause instanceof Error ? cause.message : String(cause));

/**
 * Pick a device on the left, press Scan, and see only what was deleted or
 * damaged on it.
 *
 * Colour carries state and nothing else: the window is black while idle,
 * amber while Banker works, red once deleted items are on screen. The
 * `data-state` attribute on the shell drives it (see styles.css).
 */
export function App() {
  const [discovery, setDiscovery] = useState<Discovery>({ devices: [], hints: [] });
  const [looking, setLooking] = useState(false);
  const [selected, setSelected] = useState<Device | null>(null);
  const [stage, setStage] = useState<Stage>({ kind: "idle" });
  const [scanned, setScanned] = useState<{ device: Device; summary: ScanSummary } | null>(null);
  const [sheet, setSheet] = useState<{ kind: "clone" | "restore"; device: Device } | null>(null);

  const refresh = useCallback(async () => {
    if (!nativeAvailable()) return;
    setLooking(true);
    try {
      setDiscovery(await discover());
    } catch (cause) {
      setStage({ kind: "failed", message: message(cause) });
    } finally {
      setLooking(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
    // Devices come and go; look again every few seconds while visible.
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible") void refresh();
    }, 8000);
    return () => window.clearInterval(timer);
  }, [refresh]);

  useEffect(() => {
    const stop = onProgress((progress) =>
      setStage((s) =>
        s.kind === "scanning" || s.kind === "cloning" || s.kind === "restoring" ? { ...s, progress } : s,
      ),
    );
    return () => {
      void stop.then((f) => f());
    };
  }, []);

  const start = async (device: Device) => {
    setSelected(device);
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
      void refresh();
    } catch (cause) {
      setStage({ kind: "failed", message: message(cause) });
    }
  };

  const startRestore = async (device: Device, folder: string, password: string | null) => {
    setSheet(null);
    setStage({ kind: "restoring", device, progress: null });
    try {
      setStage({ kind: "restored", device, message: await restoreClone(folder, device, password) });
    } catch (cause) {
      setStage({ kind: "failed", message: message(cause) });
    }
  };

  /** Scan a finished clone for deleted items, as if it were a backup. */
  const scanClone = (path: string, name: string) =>
    void start({ id: `clone:${path}`, kind: "backup", name: `${name} (clone)`, detail: path, path, problem: null });

  const working = stage.kind === "scanning" || stage.kind === "cloning" || stage.kind === "restoring";
  const done = stage.kind === "cloned" || stage.kind === "restored";
  const showResults = !working && !done && scanned !== null && scanned.device.id === selected?.id;
  const state = working
    ? "working"
    : done
      ? "saved"
      : showResults && scanned.summary.findings > 0
        ? "found"
        : "idle";

  return (
    <div className="shell" data-state={state}>
      <Sidebar
        discovery={discovery}
        looking={looking}
        selected={selected}
        scanningId={working ? stage.device.id : null}
        workingLabel={
          stage.kind === "cloning" ? "Cloning…" : stage.kind === "restoring" ? "Restoring…" : "Scanning…"
        }
        foundId={scanned && scanned.summary.findings > 0 ? scanned.device.id : null}
        compact={showResults}
        disabled={working}
        onSelect={(d) => {
          setSelected(d);
          if (stage.kind === "failed") setStage({ kind: "idle" });
        }}
        onRefresh={() => void refresh()}
      />
      <main className="main">
        {!nativeAvailable() && (
          <p className="banner">
            Running in a browser tab. Start the app with <code>npm run dev</code> to reach
            devices.
          </p>
        )}
        {stage.kind === "cloned" ? (
          <CloneDone
            clone={stage.clone}
            onScan={(path) => scanClone(path, stage.device.name)}
            onBack={() => setStage({ kind: "idle" })}
          />
        ) : stage.kind === "restored" ? (
          <RestoreDone message={stage.message} onBack={() => setStage({ kind: "idle" })} />
        ) : showResults ? (
          <Results
            device={scanned.device}
            summary={scanned.summary}
            onRescan={() => void start(scanned.device)}
          />
        ) : (
          <ScanPanel
            device={selected}
            stage={stage}
            onScan={(d) => void start(d)}
            onClone={(d) => setSheet({ kind: "clone", device: d })}
            onRestore={(d) => setSheet({ kind: "restore", device: d })}
            onCancel={() => void cancelScan()}
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
