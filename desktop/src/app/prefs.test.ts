import { describe, expect, it, vi } from "vitest";
import {
  coercePrefs,
  createAutoRefreshController,
  DEFAULT_PREFS,
  filterFromPrefs,
  toPrefsDto,
} from "./prefs";

describe("prefs mapping", () => {
  it("round-trips theme, locale, auto_refresh_ms, and filter", () => {
    const prefs = toPrefsDto(
      "light",
      "en",
      30_000,
      {
        range: "7d",
        window: "week",
        source: "codex",
        project_hash: "proj-1",
      },
    );
    expect(prefs.theme).toBe("light");
    expect(prefs.locale).toBe("en");
    expect(prefs.auto_refresh_ms).toBe(30_000);
    expect(prefs.range_preset).toBe("7d");
    expect(prefs.window).toBe("week");
    expect(prefs.filter.source).toBe("codex");
    expect(prefs.filter.project_hash).toBe("proj-1");
    expect(prefs.filter.since).toBeNull();
    const restored = filterFromPrefs(prefs);
    expect(restored.range).toBe("7d");
    expect(restored.source).toBe("codex");
    expect(restored.project_hash).toBe("proj-1");
  });

  it("keeps custom since/until and coerces invalid auto_refresh_ms", () => {
    const prefs = toPrefsDto("dark", "zh", 0, {
      range: "custom",
      window: "day",
      since: "2026-01-02",
      until: "2026-01-09",
    });
    expect(prefs.filter.since).toBe("2026-01-02");
    expect(filterFromPrefs(prefs).until).toBe("2026-01-09");
    expect(coercePrefs({ ...DEFAULT_PREFS, auto_refresh_ms: 12 as never }).auto_refresh_ms).toBe(0);
  });
});

describe("auto refresh interval", () => {
  it("uses the new delay on the next tick after an interval change", () => {
    vi.useFakeTimers();
    const ticks: number[] = [];
    const controller = createAutoRefreshController(30_000, () => {
      ticks.push(1);
    });
    vi.advanceTimersByTime(30_000);
    expect(ticks).toEqual([1]);
    controller.setInterval(60_000);
    vi.advanceTimersByTime(30_000);
    expect(ticks).toEqual([1]);
    vi.advanceTimersByTime(30_000);
    expect(ticks).toEqual([1, 1]);
    controller.setInterval(0);
    vi.advanceTimersByTime(120_000);
    expect(ticks).toEqual([1, 1]);
    controller.stop();
  });
});
