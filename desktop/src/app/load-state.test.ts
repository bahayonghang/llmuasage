import { waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { emptyInteractiveSnapshot } from "../test/fixtures";
import { createLoadController } from "./load-state";
import type { FilterState } from "./types";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

describe("loadDashboardProgressive", () => {
  it("increments generation, cancels old ids, starts a new interactive query, and drops stale results", async () => {
    const first = deferred<ReturnType<typeof emptyInteractiveSnapshot>>();
    const second = deferred<ReturnType<typeof emptyInteractiveSnapshot>>();
    const interactiveCalls: unknown[] = [];
    const invokeCommand = vi.fn(async (command: string, args?: Record<string, unknown>) => {
      if (command === "home_overview") {
        throw new Error("home_overview must not be called");
      }
      if (command === "cancel_queries") {
        return;
      }
      if (command === "dashboard_interactive") {
        interactiveCalls.push(args);
        if (interactiveCalls.length === 1) {
          return first.promise;
        }
        return second.promise;
      }
      throw new Error(`unexpected ${command}`);
    });
    const applied: unknown[] = [];
    const controller = createLoadController({
      invokeCommand: invokeCommand as never,
      now: () => new Date(2026, 8, 4),
      onSnapshot: (_generation, snapshot) => {
        applied.push(snapshot);
      },
    });

    const firstState: FilterState = { range: "1d" };
    const secondState: FilterState = { range: "7d" };
    const pendingFirst = controller.loadDashboardProgressive(firstState);
    expect(controller.generation).toBe(1);
    const pendingSecond = controller.loadDashboardProgressive(secondState);
    expect(controller.generation).toBe(2);

    await waitFor(() => {
      expect(invokeCommand).toHaveBeenCalledWith("cancel_queries", {
        request: { request_ids: [1] },
      });
      expect(interactiveCalls).toHaveLength(2);
    });
    const firstRequest = (interactiveCalls[0] as { request: { filter: { since: string }; window: string } })
      .request;
    const secondRequest = (interactiveCalls[1] as { request: { filter: { since: string }; window: string } })
      .request;
    expect(firstRequest.window).toBe("day");
    expect(firstRequest.filter.since).toBe("2026-09-03");
    expect(secondRequest.window).toBe("week");
    expect(secondRequest.filter.since).toBe("2026-08-29");

    const stale = emptyInteractiveSnapshot({ trends: [{ label: "stale", total_tokens: 1 }] });
    const fresh = emptyInteractiveSnapshot({ trends: [{ label: "fresh", total_tokens: 2 }] });
    first.resolve(stale);
    await pendingFirst;
    expect(applied).toEqual([]);

    second.resolve(fresh);
    await pendingSecond;
    expect(applied).toEqual([fresh]);

    const commands = invokeCommand.mock.calls.map((call) => call[0]);
    expect(commands).not.toContain("home_overview");
  });

  it("cancels the previous request_id after a completed load when the range changes", async () => {
    const invokeCommand = vi.fn(async (command: string) => {
      if (command === "cancel_queries") {
        return;
      }
      if (command === "dashboard_interactive") {
        return emptyInteractiveSnapshot();
      }
      throw new Error(`unexpected ${command}`);
    });
    const controller = createLoadController({
      invokeCommand: invokeCommand as never,
    });
    await controller.loadDashboardProgressive({ range: "1d" });
    await controller.loadDashboardProgressive({ range: "30d" });
    expect(invokeCommand).toHaveBeenCalledWith("cancel_queries", {
      request: { request_ids: [1] },
    });
    const interactive = invokeCommand.mock.calls.filter((call) => call[0] === "dashboard_interactive");
    expect(interactive).toHaveLength(2);
  });

  it("marks slow at 2s and fails at 6s while cancelling that request", async () => {
    vi.useFakeTimers();
    const invokeCommand = vi.fn(async (command: string) => {
      if (command === "cancel_queries") {
        return;
      }
      if (command === "dashboard_interactive") {
        return new Promise(() => {});
      }
      throw new Error(`unexpected ${command}`);
    });
    const onSlow = vi.fn();
    const onFail = vi.fn();
    const controller = createLoadController({
      invokeCommand: invokeCommand as never,
      onSlow,
      onFail,
    });

    const pending = controller.loadDashboardProgressive({ range: "all" });
    await Promise.resolve();
    await vi.advanceTimersByTimeAsync(2000);
    expect(onSlow).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(4000);
    await pending;
    expect(onFail).toHaveBeenCalledTimes(1);
    expect(invokeCommand).toHaveBeenCalledWith("cancel_queries", {
      request: { request_ids: [1] },
    });
  });
});
