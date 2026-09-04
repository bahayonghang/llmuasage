import { describe, expect, it } from "vitest";
import { rangeToFilterDto, RANGE_TO_TREND_WINDOW, syncOptionsFromState, windowFromState } from "./filters";
import type { FilterState } from "./types";

const NOW = new Date(2026, 8, 4);

function state(range: FilterState["range"], extra: Partial<FilterState> = {}): FilterState {
  return { range, ...extra };
}

describe("rangeToFilterDto", () => {
  it("maps 1d to yesterday..today in local calendar", () => {
    const dto = rangeToFilterDto(state("1d"), NOW);
    expect(dto.since).toBe("2026-09-03");
    expect(dto.until).toBe("2026-09-04");
    expect(dto.timezone).toBe(Intl.DateTimeFormat().resolvedOptions().timeZone);
    expect(windowFromState(state("1d"))).toBe(RANGE_TO_TREND_WINDOW["1d"]);
    expect(windowFromState(state("1d"))).toBe("day");
  });

  it("maps 7d to today-6..today", () => {
    const dto = rangeToFilterDto(state("7d"), NOW);
    expect(dto.since).toBe("2026-08-29");
    expect(dto.until).toBe("2026-09-04");
    expect(windowFromState(state("7d"))).toBe("week");
  });

  it("maps 30d to today-29..today", () => {
    const dto = rangeToFilterDto(state("30d"), NOW);
    expect(dto.since).toBe("2026-08-06");
    expect(dto.until).toBe("2026-09-04");
    expect(windowFromState(state("30d"))).toBe("month");
  });

  it("omits since/until for all", () => {
    const dto = rangeToFilterDto(state("all"), NOW);
    expect(dto.since).toBeUndefined();
    expect(dto.until).toBeUndefined();
    expect(dto.timezone).toBe(Intl.DateTimeFormat().resolvedOptions().timeZone);
    expect(windowFromState(state("all"))).toBe("all");
  });

  it("uses custom since/until inputs and keeps last window or day", () => {
    const dto = rangeToFilterDto(
      state("custom", { since: "2026-01-02", until: "2026-01-09", window: "week" }),
      NOW,
    );
    expect(dto.since).toBe("2026-01-02");
    expect(dto.until).toBe("2026-01-09");
    expect(windowFromState(state("custom", { window: "week" }))).toBe("week");
    expect(windowFromState(state("custom"))).toBe("day");
  });

  it("copies source, model, project_hash, and host_id", () => {
    const dto = rangeToFilterDto(
      state("all", {
        source: "codex",
        model: "gpt-4.1",
        project_hash: "abc",
        host_id: "host-1",
      }),
      NOW,
    );
    expect(dto.source).toBe("codex");
    expect(dto.model).toBe("gpt-4.1");
    expect(dto.project_hash).toBe("abc");
    expect(dto.host_id).toBe("host-1");
  });
});

describe("syncOptionsFromState", () => {
  it("maps five ranges and current source without rebuild", () => {
    expect(syncOptionsFromState(state("1d", { source: "codex" }))).toEqual({
      source: "codex",
      recent_days: 1,
    });
    expect(syncOptionsFromState(state("7d"))).toEqual({ recent_days: 7 });
    expect(syncOptionsFromState(state("30d"))).toEqual({ recent_days: 30 });
    expect(syncOptionsFromState(state("all", { source: "claude" }))).toEqual({ source: "claude" });
    expect(
      syncOptionsFromState(state("custom", { since: "2026-01-01", until: "2026-01-02", source: "codex" })),
    ).toEqual({ source: "codex" });

    for (const range of ["1d", "7d", "30d", "all", "custom"] as const) {
      expect(syncOptionsFromState(state(range))).not.toHaveProperty("rebuild");
      expect(syncOptionsFromState(state(range))).not.toHaveProperty("since");
      expect(syncOptionsFromState(state(range))).not.toHaveProperty("until");
    }
  });
});
