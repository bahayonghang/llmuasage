import { save } from "@tauri-apps/plugin-dialog";
import { writeTextFile } from "@tauri-apps/plugin-fs";
import { describe, expect, it, vi } from "vitest";
import { buildAnalyticsCsv } from "./csv";
import { saveAnalyticsCsv } from "./save";

const saveMock = vi.mocked(save);
const writeMock = vi.mocked(writeTextFile);

describe("saveAnalyticsCsv", () => {
  it("opens a CSV save dialog then writes the UTF-8 BOM file", async () => {
    saveMock.mockResolvedValueOnce("C:/tmp/out.csv");
    writeMock.mockResolvedValueOnce(undefined);
    const csv = buildAnalyticsCsv({}, "zh");
    await expect(saveAnalyticsCsv(csv, new Date("2026-09-04T00:00:00Z"))).resolves.toBe(
      "C:/tmp/out.csv",
    );
    expect(saveMock).toHaveBeenCalledWith({
      defaultPath: "llmusage-analytics-20260904.csv",
      filters: [{ name: "CSV", extensions: ["csv"] }],
    });
    expect(writeMock).toHaveBeenCalledWith("C:/tmp/out.csv", csv);
    expect(csv.charCodeAt(0)).toBe(0xfeff);
  });

  it("returns null when the dialog is cancelled", async () => {
    saveMock.mockResolvedValueOnce(null);
    await expect(saveAnalyticsCsv("x")).resolves.toBeNull();
    expect(writeMock).not.toHaveBeenCalled();
  });
});
