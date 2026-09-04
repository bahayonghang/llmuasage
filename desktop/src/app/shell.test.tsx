import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { describe, expect, it, vi } from "vitest";
import {
  emptyInteractiveSnapshot,
  hostRow,
  projectRow,
  runtimeInfo,
} from "../test/fixtures";
import { COPY } from "./i18n";
import { Shell } from "./shell";
import type { InteractiveSnapshot, RuntimeInfoDto } from "./types";

const invokeMock = vi.mocked(invoke);

function installInvoke(
  snapshot: InteractiveSnapshot = emptyInteractiveSnapshot(),
  info: RuntimeInfoDto = runtimeInfo(),
) {
  invokeMock.mockImplementation(async (command: string) => {
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
    throw new Error(`unexpected command ${command}`);
  });
}

function interactiveCalls() {
  return invokeMock.mock.calls.filter((call) => call[0] === "dashboard_interactive");
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

  it("paints core blocks after interactive success without home_overview", async () => {
    installInvoke();
    render(<Shell />);
    await waitFor(() => expect(screen.getByTestId("core-blocks")).toBeInTheDocument());
    expect(screen.getByTestId("overview-panel")).toBeInTheDocument();
    expect(screen.getByTestId("trends-panel")).toBeInTheDocument();
    expect(screen.getByTestId("models-panel")).toBeInTheDocument();
    expect(screen.getByTestId("sources-panel")).toBeInTheDocument();
    expect(screen.getByTestId("projects-panel")).toBeInTheDocument();
    expect(screen.getByTestId("costs-panel")).toBeInTheDocument();
    expect(screen.getByTestId("sync-center")).toBeInTheDocument();
    expect(screen.getByTestId("status-panel")).toBeInTheDocument();
    expect(screen.getByTestId("home-overview-placeholder")).toBeInTheDocument();
    expect(invokeMock.mock.calls.map((call) => call[0])).not.toContain("home_overview");
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
    expect(screen.getByTestId("status-panel")).toHaveAttribute("data-status", "lock_busy");
    expect(screen.getByTestId("status-label")).toHaveTextContent("cli:9@t");
    expect(screen.getByTestId("sync-lock-busy")).toHaveTextContent("cli:9@t");
  });

  it("shows lock_lost alert and stops writes", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string) => {
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
      throw new Error(`unexpected command ${command}`);
    });
    render(<Shell />);
    await waitFor(() => expect(screen.getByTestId("sync-button")).toBeInTheDocument());
    await user.click(screen.getByTestId("sync-button"));
    await waitFor(() => expect(screen.getByTestId("lock-lost-alert")).toBeInTheDocument());
    expect(screen.getByTestId("status-panel")).toHaveAttribute("data-status", "lock_lost");
    expect(screen.getByTestId("sync-button")).toBeDisabled();
  });
});
