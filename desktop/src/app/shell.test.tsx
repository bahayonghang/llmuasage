import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { describe, expect, it, vi } from "vitest";
import {
  defaultSecondaryPayload,
  emptyInteractiveSnapshot,
  handleDesktopOpsCommand,
  hostRow,
  projectRow,
  runtimeInfo,
} from "../test/fixtures";
import { COPY } from "./i18n";
import { LOGS_NAVIGATE_EVENT } from "./secondary";
import { Shell } from "./shell";
import type { InteractiveSnapshot, RuntimeInfoDto } from "./types";

const invokeMock = vi.mocked(invoke);

const SECONDARY_COMMANDS = new Set([
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

function installInvoke(
  snapshot: InteractiveSnapshot = emptyInteractiveSnapshot(),
  info: RuntimeInfoDto = runtimeInfo(),
  secondary: Record<string, unknown> = {},
) {
  invokeMock.mockImplementation(async (command: string, args?: unknown) => {
    const ops = handleDesktopOpsCommand(command, args);
    if (ops !== undefined) {
      return ops;
    }
    if (command === "runtime_info") {
      return info;
    }
    if (command === "dashboard_interactive") {
      return snapshot;
    }
    if (command === "cancel_queries") {
      return null;
    }
    if (command === "start_sync") {
      return {
        job_id: "job-1",
        status: "running",
        summary: null,
        error: null,
        started_at: "2026-09-04T00:00:00Z",
        finished_at: null,
      };
    }
    if (command === "cancel_job") {
      return true;
    }
    if (command === "job_snapshot") {
      return null;
    }
    if (SECONDARY_COMMANDS.has(command)) {
      return command in secondary ? secondary[command] : defaultSecondaryPayload(command);
    }
    throw new Error(`unexpected command ${command}`);
  });
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function interactiveCalls() {
  return invokeMock.mock.calls.filter((call) => call[0] === "dashboard_interactive");
}

function lastCall(command: string) {
  return invokeMock.mock.calls.filter((call) => call[0] === command).at(-1);
}

describe("Shell", () => {
  it("hides hosts when length is 0 or 1 and shows sources; shows hosts at length 2", async () => {
    const user = userEvent.setup();
    const snapshot = emptyInteractiveSnapshot({ hosts: [] });
    installInvoke(snapshot);
    const { unmount } = render(<Shell />);
    await waitFor(() => expect(screen.getByTestId("sources-panel")).toBeInTheDocument());
    expect(screen.queryByTestId("hosts-panel")).not.toBeInTheDocument();
    unmount();

    installInvoke(emptyInteractiveSnapshot({ hosts: [hostRow("only")] }));
    const second = render(<Shell />);
    await waitFor(() => expect(screen.getByTestId("sources-panel")).toBeInTheDocument());
    expect(screen.queryByTestId("hosts-panel")).not.toBeInTheDocument();
    second.unmount();

    installInvoke(
      emptyInteractiveSnapshot({ hosts: [hostRow("a", "Alpha"), hostRow("b", "Beta")] }),
    );
    render(<Shell />);
    await waitFor(() => expect(screen.getByTestId("hosts-panel")).toBeInTheDocument());
    expect(screen.getByTestId("sources-panel")).toBeInTheDocument();
    await user.click(screen.getByTestId("host-row-b"));
    await waitFor(() => {
      const last = interactiveCalls().at(-1)?.[1] as {
        request: { filter: { host_id?: string } };
      };
      expect(last.request.filter.host_id).toBe("b");
    });
  });

  it("writes project_hash on project click and reloads interactive", async () => {
    const user = userEvent.setup();
    installInvoke(
      emptyInteractiveSnapshot({
        projects: [projectRow("proj-1", "Alpha")],
      }),
    );
    render(<Shell />);
    await waitFor(() => expect(screen.getByTestId("project-row-proj-1")).toBeInTheDocument());
    await user.click(screen.getByTestId("project-row-proj-1"));
    await waitFor(() => {
      const last = interactiveCalls().at(-1)?.[1] as {
        request: { filter: { project_hash?: string } };
      };
      expect(last.request.filter.project_hash).toBe("proj-1");
    });
  });

  it("paints core blocks after interactive success without home_overview in the core request", async () => {
    installInvoke();
    render(<Shell />);
    await waitFor(() => expect(screen.getByTestId("core-blocks")).toBeInTheDocument());
    expect(screen.getByTestId("overview-panel")).toBeInTheDocument();
    expect(screen.getByTestId("hero")).toBeInTheDocument();
    expect(screen.getByTestId("trends-panel")).toBeInTheDocument();
    expect(screen.getByTestId("models-panel")).toBeInTheDocument();
    expect(screen.getByTestId("sources-panel")).toBeInTheDocument();
    expect(screen.getByTestId("projects-panel")).toBeInTheDocument();
    expect(screen.getByTestId("costs-panel")).toBeInTheDocument();
    expect(screen.getByTestId("sync-center")).toBeInTheDocument();
    expect(screen.getByTestId("status-panel")).toBeInTheDocument();
    const interactive = interactiveCalls()[0]?.[1] as { request: Record<string, unknown> };
    expect(interactive.request).not.toHaveProperty("home_overview");
    await waitFor(() => expect(invokeMock.mock.calls.map((call) => call[0])).toContain("home_overview"));
    const names = invokeMock.mock.calls.map((call) => call[0]);
    expect(names.indexOf("dashboard_interactive")).toBeLessThan(names.indexOf("home_overview"));
  });

  it("switches sidebar copy between zh and en", async () => {
    const user = userEvent.setup();
    installInvoke();
    render(<Shell />);
    await waitFor(() => expect(screen.getByTestId("nav-overview")).toHaveTextContent(COPY.zh.navUsage));
    expect(screen.getByTestId("nav-quota")).toHaveTextContent(COPY.zh.navQuota);
    await user.click(screen.getByTestId("locale-toggle"));
    expect(screen.getByTestId("nav-overview")).toHaveTextContent(COPY.en.navUsage);
    expect(screen.getByTestId("nav-trends")).toHaveTextContent(COPY.en.navTrend);
    expect(screen.getByTestId("nav-quota")).toHaveTextContent(COPY.en.navQuota);
  });

  it("starts sync with mapped SyncStartDto and no rebuild", async () => {
    const user = userEvent.setup();
    installInvoke();
    render(<Shell />);
    await waitFor(() => expect(screen.getByTestId("sync-button")).toBeInTheDocument());
    await user.click(screen.getByTestId("range-7d"));
    await user.click(screen.getByTestId("sync-button"));
    await waitFor(() => {
      const call = invokeMock.mock.calls.find((entry) => entry[0] === "start_sync");
      expect(call?.[1]).toEqual({ request: { recent_days: 7 } });
      expect(call?.[1]).not.toHaveProperty("rebuild");
      expect((call?.[1] as { request: object }).request).not.toHaveProperty("rebuild");
    });
  });

  it("shows root_dir in the sidebar", async () => {
    installInvoke();
    render(<Shell />);
    await waitFor(() =>
      expect(screen.getByTestId("root-dir")).toHaveTextContent("C:\\tmp\\llmusage-home"),
    );
  });

  it("cancels a running job through cancel_job", async () => {
    const user = userEvent.setup();
    installInvoke();
    render(<Shell />);
    await waitFor(() => expect(screen.getByTestId("sync-button")).toBeInTheDocument());
    await user.click(screen.getByTestId("sync-button"));
    await waitFor(() => expect(screen.getByTestId("cancel-sync-button")).toBeInTheDocument());
    expect(screen.getByTestId("sync-running")).toBeInTheDocument();
    await user.click(screen.getByTestId("cancel-sync-button"));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("cancel_job", { id: "job-1" });
    });
  });

  it("shows lock_busy with holder and disables sync", async () => {
    installInvoke(
      emptyInteractiveSnapshot(),
      runtimeInfo({
        lock: {
          holder_pid: 9,
          holder_kind: "cli",
          acquired_at: "t",
          lease_expires_at: "t2",
          updated_at: "t",
        },
      }),
    );
    render(<Shell />);
    await waitFor(() => expect(screen.getByTestId("sync-button")).toBeDisabled());
    await waitFor(() => expect(screen.getByTestId("status-panel")).toHaveAttribute("data-status", "lock_busy"));
    expect(screen.getByTestId("status-label")).toHaveTextContent("cli:9@t");
    expect(screen.getByTestId("sync-lock-busy")).toHaveTextContent("cli:9@t");
  });

  it("shows lock_lost alert and stops writes", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string, args?: unknown) => {
      const ops = handleDesktopOpsCommand(command, args);
      if (ops !== undefined) {
        return ops;
      }
      if (command === "runtime_info") {
        return runtimeInfo();
      }
      if (command === "dashboard_interactive") {
        return emptyInteractiveSnapshot();
      }
      if (command === "cancel_queries") {
        return null;
      }
      if (command === "start_sync") {
        throw { code: "lock_lost", message: "worker lock lost" };
      }
      if (SECONDARY_COMMANDS.has(command)) {
        return defaultSecondaryPayload(command);
      }
      throw new Error(`unexpected command ${command}`);
    });
    render(<Shell />);
    await waitFor(() => expect(screen.getByTestId("sync-button")).toBeInTheDocument());
    await user.click(screen.getByTestId("sync-button"));
    await waitFor(() => expect(screen.getByTestId("lock-lost-alert")).toBeInTheDocument());
    expect(screen.getByTestId("status-panel")).toHaveAttribute("data-status", "lock_lost");
    expect(screen.getByTestId("sync-button")).toBeDisabled();
  });

  it("invokes home_overview only after the core snapshot exists", async () => {
    const core = deferred<InteractiveSnapshot>();
    invokeMock.mockImplementation(async (command: string, args?: unknown) => {
      const ops = handleDesktopOpsCommand(command, args);
      if (ops !== undefined) {
        return ops;
      }
      if (command === "runtime_info") {
        return runtimeInfo();
      }
      if (command === "dashboard_interactive") {
        return core.promise;
      }
      if (command === "cancel_queries") {
        return null;
      }
      if (SECONDARY_COMMANDS.has(command)) {
        return defaultSecondaryPayload(command);
      }
      throw new Error(`unexpected command ${command}`);
    });
    render(<Shell />);
    await waitFor(() => expect(interactiveCalls()).toHaveLength(1));
    expect(invokeMock.mock.calls.map((call) => call[0])).not.toContain("home_overview");
    expect(screen.queryByTestId("core-blocks")).not.toBeInTheDocument();
    core.resolve(emptyInteractiveSnapshot());
    await waitFor(() => expect(screen.getByTestId("summary-cards")).toBeInTheDocument());
    expect(screen.getByTestId("overview-panel")).toBeInTheDocument();
    expect(screen.getByTestId("hero")).toBeInTheDocument();
    expect(invokeMock.mock.calls.map((call) => call[0])).toContain("home_overview");
  });

  it("degrades six cards on home_overview failure and keeps overview and hero", async () => {
    invokeMock.mockImplementation(async (command: string, args?: unknown) => {
      const ops = handleDesktopOpsCommand(command, args);
      if (ops !== undefined) {
        return ops;
      }
      if (command === "runtime_info") {
        return runtimeInfo();
      }
      if (command === "dashboard_interactive") {
        return emptyInteractiveSnapshot();
      }
      if (command === "cancel_queries") {
        return null;
      }
      if (command === "home_overview") {
        throw { code: "timeout", message: "timed out" };
      }
      if (SECONDARY_COMMANDS.has(command)) {
        return defaultSecondaryPayload(command);
      }
      throw new Error(`unexpected command ${command}`);
    });
    render(<Shell />);
    await waitFor(() => expect(screen.getByTestId("summary-cards")).toHaveAttribute("data-state", "degraded"));
    await waitFor(() => expect(screen.getByTestId("heatmap-date-2026-09-01")).toBeInTheDocument());
    expect(screen.getByTestId("overview-panel")).toBeInTheDocument();
    expect(screen.getByTestId("hero")).toBeInTheDocument();
    expect(screen.getByTestId("summary-card-sessions")).toHaveTextContent("—");
    expect(screen.getByTestId("heatmap-panel")).toBeInTheDocument();
  });

  it("keeps other secondary sections when one section fails", async () => {
    invokeMock.mockImplementation(async (command: string, args?: unknown) => {
      const ops = handleDesktopOpsCommand(command, args);
      if (ops !== undefined) {
        return ops;
      }
      if (command === "runtime_info") {
        return runtimeInfo();
      }
      if (command === "dashboard_interactive") {
        return emptyInteractiveSnapshot();
      }
      if (command === "cancel_queries") {
        return null;
      }
      if (command === "tools") {
        throw { code: "timeout", message: "tools timed out" };
      }
      if (SECONDARY_COMMANDS.has(command)) {
        return defaultSecondaryPayload(command);
      }
      throw new Error(`unexpected command ${command}`);
    });
    render(<Shell />);
    await waitFor(() => expect(screen.getByTestId("heatmap-date-2026-09-01")).toBeInTheDocument());
    expect(screen.getByTestId("tools-panel")).toHaveAttribute("data-support", "degraded");
    expect(screen.getByTestId("summary-cards")).toHaveAttribute("data-state", "ready");
    expect(screen.getByTestId("overview-panel")).toBeInTheDocument();
    expect(screen.getByTestId("hero")).toBeInTheDocument();
  });

  it("cancels secondary request_ids when the range changes", async () => {
    const user = userEvent.setup();
    installInvoke();
    render(<Shell />);
    await waitFor(() => {
      const secondary = invokeMock.mock.calls.filter((call) =>
        SECONDARY_COMMANDS.has(String(call[0])),
      );
      expect(new Set(secondary.map((call) => call[0])).size).toBe(SECONDARY_COMMANDS.size);
    });
    const secondaryIds = invokeMock.mock.calls
      .filter((call) => SECONDARY_COMMANDS.has(String(call[0])))
      .map((call) => (call[1] as { request: { request_id: number } }).request.request_id);
    const before = invokeMock.mock.calls.length;
    await user.click(screen.getByTestId("range-7d"));
    await waitFor(() => {
      const cancel = invokeMock.mock.calls
        .slice(before)
        .find((call) => call[0] === "cancel_queries");
      expect(cancel).toBeTruthy();
      const ids = (cancel?.[1] as { request: { request_ids: number[] } }).request.request_ids;
      expect(ids).toEqual(expect.arrayContaining(secondaryIds));
    });
  });

  it("reloads only explorer when an explorer control changes", async () => {
    const user = userEvent.setup();
    installInvoke();
    render(<Shell />);
    await waitFor(() => expect(screen.getByTestId("explorer-metric")).toBeInTheDocument());
    const before = invokeMock.mock.calls.length;
    await user.selectOptions(screen.getByTestId("explorer-metric"), "calls");
    await waitFor(() => {
      const after = invokeMock.mock.calls.slice(before);
      const names = after.map((call) => call[0]).filter((name) => name !== "cancel_queries");
      expect(names).toEqual(["explorer"]);
    });
    const request = (lastCall("explorer")?.[1] as { request: { metric: string; include_non_tool: boolean } })
      .request;
    expect(request.metric).toBe("calls");
  });

  it("sends include_non_tool false on the explorer DTO", async () => {
    const user = userEvent.setup();
    installInvoke();
    render(<Shell />);
    await waitFor(() => expect(screen.getByTestId("explorer-include-non-tool")).toBeChecked());
    const before = invokeMock.mock.calls.length;
    await user.click(screen.getByTestId("explorer-include-non-tool"));
    await waitFor(() => {
      const after = invokeMock.mock.calls.slice(before);
      expect(after.map((call) => call[0]).filter((name) => name !== "cancel_queries")).toEqual(["explorer"]);
    });
    const request = (lastCall("explorer")?.[1] as { request: { include_non_tool: boolean } }).request;
    expect(request.include_non_tool).toBe(false);
  });

  it("sets custom since=until on heatmap date click and restores the previous range on the second click", async () => {
    const user = userEvent.setup();
    installInvoke();
    render(<Shell />);
    await waitFor(() => expect(screen.getByTestId("heatmap-date-2026-09-01")).toBeInTheDocument());
    await user.click(screen.getByTestId("heatmap-date-2026-09-01"));
    await waitFor(() => {
      const last = interactiveCalls().at(-1)?.[1] as {
        request: { filter: { since?: string; until?: string } };
      };
      expect(last.request.filter.since).toBe("2026-09-01");
      expect(last.request.filter.until).toBe("2026-09-01");
    });
    expect(screen.getByTestId("range-custom")).toHaveClass("active");
    await user.click(await screen.findByTestId("heatmap-date-2026-09-01"));
    await waitFor(() => {
      const last = interactiveCalls().at(-1)?.[1] as { request: { filter: { since?: string; until?: string } } };
      expect(last.request.filter.since).toBeUndefined();
      expect(last.request.filter.until).toBeUndefined();
    });
  });


  it("emits a logs intent with the session key when a session row is clicked", async () => {
    const user = userEvent.setup();
    const seen: unknown[] = [];
    const handler = (event: Event) => {
      seen.push((event as CustomEvent).detail);
    };
    window.addEventListener(LOGS_NAVIGATE_EVENT, handler);
    installInvoke();
    render(<Shell />);
    await waitFor(() => expect(screen.getByTestId("session-row-sess-1")).toBeInTheDocument());
    await user.click(screen.getByTestId("session-row-sess-1"));
    expect(seen).toEqual([{ session: "sess-1" }]);
    window.removeEventListener(LOGS_NAVIGATE_EVENT, handler);
  });

  it("drops stale secondary heatmap results after a newer generation", async () => {
    const firstHeatmap = deferred<unknown>();
    let heatmapCalls = 0;
    invokeMock.mockImplementation(async (command: string, args?: unknown) => {
      const ops = handleDesktopOpsCommand(command, args);
      if (ops !== undefined) {
        return ops;
      }
      if (command === "runtime_info") {
        return runtimeInfo();
      }
      if (command === "dashboard_interactive") {
        return emptyInteractiveSnapshot();
      }
      if (command === "cancel_queries") {
        return null;
      }
      if (command === "heatmap") {
        heatmapCalls += 1;
        if (heatmapCalls === 1) {
          return firstHeatmap.promise;
        }
        return [{ date: "2026-08-01", event_count: 1, total_tokens: 5 }];
      }
      if (SECONDARY_COMMANDS.has(command)) {
        return defaultSecondaryPayload(command);
      }
      throw new Error(`unexpected command ${command}`);
    });
    const user = userEvent.setup();
    render(<Shell />);
    await waitFor(() => expect(screen.getByTestId("core-blocks")).toBeInTheDocument());
    await waitFor(() => expect(heatmapCalls).toBeGreaterThan(0));
    await user.click(screen.getByTestId("range-7d"));
    await waitFor(() => expect(heatmapCalls).toBeGreaterThan(1));
    firstHeatmap.resolve([{ date: "2026-01-01", event_count: 9, total_tokens: 9 }]);
    await waitFor(() => expect(screen.getByTestId("heatmap-date-2026-08-01")).toBeInTheDocument());
    expect(screen.queryByTestId("heatmap-date-2026-01-01")).not.toBeInTheDocument();
  });

  it("sends LogsDto.session from a session ranking jump", async () => {
    const user = userEvent.setup();
    installInvoke();
    render(<Shell />);
    await waitFor(() => expect(screen.getByTestId("session-row-sess-1")).toBeInTheDocument());
    await user.click(screen.getByTestId("session-row-sess-1"));
    await waitFor(() => {
      const logsCalls = invokeMock.mock.calls.filter((call) => call[0] === "logs");
      const withSession = logsCalls.find(
        (call) => (call[1] as { request: { session?: string } }).request.session === "sess-1",
      );
      expect(withSession).toBeTruthy();
      expect((withSession?.[1] as { request: { page_size: number } }).request.page_size).toBe(20);
    });
  });

  it("restores theme, locale, refresh interval, and filter from load_prefs", async () => {
    invokeMock.mockImplementation(async (command: string, args?: unknown) => {
      const ops = handleDesktopOpsCommand(command, args);
      if (command === "load_prefs") {
        return {
          theme: "light",
          locale: "en",
          auto_refresh_ms: 60_000,
          filter: {
            source: "codex",
            model: null,
            since: null,
            until: null,
            project_hash: null,
            host_id: null,
            timezone: null,
          },
          window: "week",
          range_preset: "7d",
        };
      }
      if (ops !== undefined) {
        return ops;
      }
      if (command === "runtime_info") {
        return runtimeInfo();
      }
      if (command === "dashboard_interactive") {
        return emptyInteractiveSnapshot();
      }
      if (command === "cancel_queries") {
        return null;
      }
      if (SECONDARY_COMMANDS.has(command)) {
        return defaultSecondaryPayload(command);
      }
      throw new Error(`unexpected command ${command}`);
    });
    render(<Shell />);
    await waitFor(() => expect(document.documentElement.getAttribute("data-theme")).toBe("light"));
    await waitFor(() => expect(screen.getByTestId("nav-overview")).toHaveTextContent(COPY.en.navUsage));
    await waitFor(() => expect(screen.getByTestId("auto-refresh-60000")).toHaveClass("active"));
    await waitFor(() => expect(screen.getByTestId("range-7d")).toHaveClass("active"));
    await waitFor(() => {
      const last = interactiveCalls().at(-1)?.[1] as { request: { window: string; filter: { source?: string } } };
      expect(last.request.window).toBe("week");
      expect(last.request.filter.source).toBe("codex");
    });
  });

  it("keeps the core snapshot when fetch_quota fails", async () => {
    invokeMock.mockImplementation(async (command: string, args?: unknown) => {
      if (command === "fetch_quota") {
        throw { code: "error", message: "quota down" };
      }
      const ops = handleDesktopOpsCommand(command, args);
      if (ops !== undefined) {
        return ops;
      }
      if (command === "runtime_info") {
        return runtimeInfo();
      }
      if (command === "dashboard_interactive") {
        return emptyInteractiveSnapshot();
      }
      if (command === "cancel_queries") {
        return null;
      }
      if (SECONDARY_COMMANDS.has(command)) {
        return defaultSecondaryPayload(command);
      }
      throw new Error(`unexpected command ${command}`);
    });
    const interactiveBefore = interactiveCalls().length;
    render(<Shell />);
    await waitFor(() => expect(screen.getByTestId("core-blocks")).toBeInTheDocument());
    await waitFor(() => expect(screen.getByTestId("quota-error")).toBeInTheDocument());
    expect(screen.getByTestId("overview-panel")).toBeInTheDocument();
    expect(screen.getByTestId("heatmap-panel")).toBeInTheDocument();
    expect(screen.getByTestId("status-panel")).toBeInTheDocument();
    expect(interactiveCalls().length).toBeGreaterThan(interactiveBefore);
    const afterError = interactiveCalls().length;
    await waitFor(() => expect(screen.getByTestId("quota-error")).toHaveTextContent("quota down"));
    expect(interactiveCalls().length).toBe(afterError);
    expect(screen.getByTestId("core-blocks")).toBeInTheDocument();
  });
});
