import { useCallback, useEffect, useState } from "react";

import { Results } from "./components/Results";
import { ScanPanel } from "./components/ScanPanel";
import { Sidebar } from "./components/Sidebar";
import {
  cancelScan,
  discover,
  nativeAvailable,
  onProgress,
  scan,
  type Device,
  type Discovery,
  type Progress,
  type ScanSummary,
} from "./lib/native";

type Stage =
  | { kind: "idle" }
  | { kind: "scanning"; progress: Progress | null }
  | { kind: "done"; summary: ScanSummary }
  | { kind: "failed"; message: string };

const message = (cause: unknown) => (cause instanceof Error ? cause.message : String(cause));

/**
 * Pick a device on the left, press Scan, and see only what was deleted or
 * damaged on it, laid out on a timeline.
 */
export function App() {
  const [discovery, setDiscovery] = useState<Discovery>({ devices: [], hints: [] });
  const [looking, setLooking] = useState(false);
  const [selected, setSelected] = useState<Device | null>(null);
  const [stage, setStage] = useState<Stage>({ kind: "idle" });
  const [scanned, setScanned] = useState<Device | null>(null);

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
    // Devices come and go; look again every few seconds while idle.
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible") void refresh();
    }, 8000);
    return () => window.clearInterval(timer);
  }, [refresh]);

  useEffect(() => {
    const stop = onProgress((progress) =>
      setStage((s) => (s.kind === "scanning" ? { kind: "scanning", progress } : s)),
    );
    return () => {
      void stop.then((f) => f());
    };
  }, []);

  const start = async (device: Device) => {
    setSelected(device);
    setStage({ kind: "scanning", progress: null });
    try {
      const summary = await scan(device);
      setScanned(device);
      setStage({ kind: "done", summary });
    } catch (cause) {
      setStage({ kind: "failed", message: message(cause) });
    }
  };

  const busy = stage.kind === "scanning";
  const showResults = stage.kind === "done" && scanned !== null && scanned.id === selected?.id;

  return (
    <div className="shell">
      <Sidebar
        discovery={discovery}
        looking={looking}
        selected={selected}
        disabled={busy}
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
        {showResults && stage.kind === "done" ? (
          <Results
            device={scanned}
            summary={stage.summary}
            onRescan={() => void start(scanned)}
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
