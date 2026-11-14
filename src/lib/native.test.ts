import { describe, expect, it } from "vitest";

import { bytes, count, daysInMonth, finer, when } from "./format";
import { discover, nativeAvailable, onProgress } from "./native";

describe("native bridge", () => {
  it("reports that it is not inside the Tauri window under test", () => {
    expect(nativeAvailable()).toBe(false);
  });

  it("refuses with a useful message instead of hanging", async () => {
    await expect(discover()).rejects.toThrow(/npm run dev/);
  });

  it("does not throw when listening outside the window", async () => {
    const stop = await onProgress(() => {});
    expect(typeof stop).toBe("function");
  });
});

describe("format", () => {
  it("drills to the next zoom and stops at minutes", () => {
    expect(finer("year")).toBe("month");
    expect(finer("hour")).toBe("minute");
    expect(finer("minute")).toBe("minute");
  });

  it("sizes and counts read naturally", () => {
    expect(bytes(512)).toBe("512 B");
    expect(bytes(1536)).toBe("1.5 KB");
    expect(bytes(50 * 1024 * 1024)).toBe("50 MB");
    expect(count(1, "file")).toBe("1 file");
    expect(count(2, "file")).toBe("2 files");
  });

  it("marks estimated times and unknown dates", () => {
    expect(when(null, false)).toBe("Date unknown");
    expect(when(0, true).startsWith("≈ ")).toBe(true);
  });

  it("knows month lengths", () => {
    expect(daysInMonth(2024, 1)).toBe(29);
    expect(daysInMonth(2025, 1)).toBe(28);
  });
});
