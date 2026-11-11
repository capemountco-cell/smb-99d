import { useCallback, useEffect, useState } from "react";

import { Results } from "./components/Results";
import { ScanPanel, type Stage } from "./components/ScanPanel";
import { Sidebar } from "./components/Sidebar";
import {
  cancelScan,
  discover,
  nativeAvailable,
  onProgress,
  scan,
  type Device,
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
      setStage((s) => (s.kind === "scanning" ? { ...s, progress } : s)),
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

  const working = stage.kind === "scanning";
  const showResults = !working && scanned !== null && scanned.device.id === selected?.id;
  const state = working ? "working" : showResults && scanned.summary.findings > 0 ? "found" : "idle";

  return (
    <div className="shell" data-state={state}>
      <Sidebar
        discovery={discovery}
        looking={looking}
        selected={selected}
        scanningId={working ? stage.device.id : null}
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
        {showResults ? (
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
            onCancel={() => void cancelScan()}
          />
        )}
      </main>
    </div>
  );
}
