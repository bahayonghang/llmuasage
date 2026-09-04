import { describe, expect, it, vi } from "vitest";
import {
  DEFAULT_EXPLORER_QUERY,
  SECONDARY_CONCURRENCY,
  SECONDARY_SECTIONS,
  applyHeatmapDateClick,
  loadSecondarySections,
  runLoadersWithConcurrency,
  shouldAcceptSecondaryResult,
  toExplorerDto,
} from "./secondary";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe("SECONDARY_SECTIONS", () => {
  it("matches the live dashboard secondary list", () => {
    expect([...SECONDARY_SECTIONS]).toEqual([
      "activity",
      "tools",
      "optimize",
      "explorer",
      "compare",
      "home_overview",
      "heatmap",
      "trends_daily",
      "top_sessions",
      "hour_of_week",
    ]);
  });
});

describe("runLoadersWithConcurrency", () => {
  it("keeps at most two loaders in flight", async () => {
    let inflight = 0;
    let maxInflight = 0;
    const started: string[] = [];
    const loaders = Object.fromEntries(
      SECONDARY_SECTIONS.map((section) => [
        section,
        async () => {
          inflight += 1;
          maxInflight = Math.max(maxInflight, inflight);
          started.push(section);
          await new Promise((resolve) => {
            setTimeout(resolve, 15);
          });
          inflight -= 1;
          return { section };
        },
      ]),
    );

    const settled: string[] = [];
    await runLoadersWithConcurrency(loaders, SECONDARY_CONCURRENCY, (section) => {
      settled.push(section);
    });
    expect(maxInflight).toBe(2);
    expect(started).toHaveLength(SECONDARY_SECTIONS.length);
    expect(settled).toHaveLength(SECONDARY_SECTIONS.length);
  });

  it("settles remaining sections when one loader fails", async () => {
    const loaders = {
      activity: async () => ({ ok: true }),
      tools: async () => {
        throw new Error("tools failed");
      },
      optimize: async () => ({ ok: true }),
    };
    const results: { section: string; error: unknown }[] = [];
    await runLoadersWithConcurrency(loaders, 2, (section, _payload, error) => {
      results.push({ section, error });
    });
    expect(results.map((row) => row.section).sort()).toEqual(["activity", "optimize", "tools"]);
    expect(results.find((row) => row.section === "tools")?.error).toBeInstanceOf(Error);
    expect(results.filter((row) => row.section !== "tools").every((row) => !row.error)).toBe(true);
  });

  it("drops stale results after generation changes", async () => {
    let current = 1;
    const heatmap = deferred<unknown>();
    const activity = deferred<unknown>();
    const applied: string[] = [];
    const pending = runLoadersWithConcurrency(
      {
        heatmap: () => heatmap.promise,
        activity: () => activity.promise,
      },
      2,
      (section) => {
        if (!shouldAcceptSecondaryResult(1, current)) {
          return;
        }
        applied.push(section);
      },
    );
    current = 2;
    heatmap.resolve({ rows: [{ date: "stale" }] });
    activity.resolve({ ok: true });
    await pending;
    expect(applied).toEqual([]);
  });
});

describe("loadSecondarySections", () => {
  it("drops stale section payloads when generation no longer matches", async () => {
    let current = 5;
    const heatmap = deferred<unknown>();
    const invokeCommand = vi.fn(async (command: string) => {
      if (command === "heatmap") {
        return heatmap.promise;
      }
      return { command };
    });
    const applied: string[] = [];
    const pending = loadSecondarySections({
      generation: 5,
      isCurrent: (generation) => generation === current,
      filter: { range: "all" },
      explorer: DEFAULT_EXPLORER_QUERY,
      sessionsSort: "tokens",
      invokeCommand: invokeCommand as never,
      onResult: (section) => {
        applied.push(section);
      },
    });
    current = 6;
    heatmap.resolve([{ date: "stale" }]);
    await pending;
    expect(applied).toEqual([]);
  });

  it("does not invoke remaining sections after generation changes", async () => {
    let current = 1;
    const activity = deferred<unknown>();
    const tools = deferred<unknown>();
    const invokeCommand = vi.fn(async (command: string) => {
      if (command === "activity") {
        return activity.promise;
      }
      if (command === "tools") {
        return tools.promise;
      }
      return { command };
    });
    const pending = loadSecondarySections({
      generation: 1,
      isCurrent: (generation) => generation === current,
      filter: { range: "all" },
      explorer: DEFAULT_EXPLORER_QUERY,
      sessionsSort: "tokens",
      invokeCommand: invokeCommand as never,
      onResult: () => undefined,
    });
    await vi.waitFor(() => expect(invokeCommand).toHaveBeenCalledTimes(2));
    current = 2;
    activity.resolve({});
    tools.resolve({});
    await pending;
    expect(invokeCommand.mock.calls.map((call) => call[0])).toEqual(["activity", "tools"]);
  });

  it("reports request_id for each invoked secondary command", async () => {
    const ids: number[] = [];
    let next = 40;
    const invokeCommand = vi.fn(async () => ({}));
    await loadSecondarySections({
      generation: 1,
      isCurrent: () => true,
      filter: { range: "all" },
      explorer: DEFAULT_EXPLORER_QUERY,
      sessionsSort: "tokens",
      invokeCommand: invokeCommand as never,
      allocateRequestId: () => {
        next += 1;
        return next;
      },
      onRequestId: (id) => {
        ids.push(id);
      },
      onResult: () => undefined,
    });
    expect(ids).toEqual([41, 42, 43, 44, 45, 46, 47, 48, 49, 50]);
    expect(invokeCommand).toHaveBeenCalledTimes(SECONDARY_SECTIONS.length);
    for (const call of invokeCommand.mock.calls) {
      const args = call as unknown as [string, { request: { request_id: number } }];
      expect(args[1].request.request_id).toBeGreaterThan(40);
    }
  });
});

describe("applyHeatmapDateClick", () => {
  it("sets custom since=until and restores the previous range on the second click", () => {
    const current = { range: "7d" as const, window: "week", source: "codex" };
    const first = applyHeatmapDateClick(current, { date: null, previous: null }, "2026-09-01");
    expect(first.filter).toEqual({
      range: "custom",
      window: "week",
      source: "codex",
      since: "2026-09-01",
      until: "2026-09-01",
    });
    expect(first.drill.date).toBe("2026-09-01");
    const second = applyHeatmapDateClick(first.filter, first.drill, "2026-09-01");
    expect(second.filter).toEqual(current);
    expect(second.drill).toEqual({ date: null, previous: null });
  });
});

describe("toExplorerDto", () => {
  it("sends include_non_tool false as a boolean field", () => {
    const dto = toExplorerDto(
      { source: "codex" },
      9,
      { ...DEFAULT_EXPLORER_QUERY, include_non_tool: false, session_id: "" },
    );
    expect(dto.include_non_tool).toBe(false);
    expect(dto.session_id).toBeUndefined();
    expect(dto.request_id).toBe(9);
  });
});
