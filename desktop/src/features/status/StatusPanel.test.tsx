import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { COPY } from "../../app/i18n";
import { StatusPanel, deriveRuntimeStatus } from "./StatusPanel";

describe("StatusPanel", () => {
  it("derives idle, running, failed, lock_busy, and lock_lost", () => {
    expect(deriveRuntimeStatus({})).toBe("idle");
    expect(deriveRuntimeStatus({ jobStatus: "running" })).toBe("running");
    expect(deriveRuntimeStatus({ jobStatus: "failed" })).toBe("failed");
    expect(deriveRuntimeStatus({ lastRunStatus: "failed" })).toBe("failed");
    expect(deriveRuntimeStatus({ lastErrorCode: "lock_busy", lockPresent: true })).toBe("lock_busy");
    expect(deriveRuntimeStatus({ lockPresent: true })).toBe("lock_busy");
    expect(deriveRuntimeStatus({ lastErrorCode: "lock_lost" })).toBe("lock_lost");
  });

  it("shows copy for each status, lock_lost alert, and diagnostics empty vs present", () => {
    const copy = COPY.zh;
    const { rerender } = render(
      <StatusPanel status="idle" diagnostics={null} copy={copy} />,
    );
    expect(screen.getByTestId("status-label")).toHaveTextContent(copy.statusIdle);
    expect(screen.getByTestId("diagnostics-empty")).toHaveTextContent(copy.diagnosticsEmpty);

    rerender(<StatusPanel status="running" diagnostics={null} copy={copy} />);
    expect(screen.getByTestId("status-label")).toHaveTextContent(copy.statusRunning);

    rerender(<StatusPanel status="failed" diagnostics={null} copy={copy} />);
    expect(screen.getByTestId("status-label")).toHaveTextContent(copy.statusFailed);

    rerender(
      <StatusPanel status="lock_busy" holder="cli:1@t" diagnostics={null} copy={copy} />,
    );
    expect(screen.getByTestId("status-label")).toHaveTextContent(copy.statusLockBusy);
    expect(screen.getByTestId("status-label")).toHaveTextContent("cli:1@t");
    expect(screen.queryByTestId("lock-lost-alert")).not.toBeInTheDocument();

    rerender(
      <StatusPanel
        status="lock_lost"
        diagnostics={{ archive_root: "C:\\data", by_source: [], recent_failures: [] }}
        copy={copy}
      />,
    );
    expect(screen.getByTestId("status-label")).toHaveTextContent(copy.statusLockLost);
    expect(screen.getByTestId("lock-lost-alert")).toHaveTextContent(copy.lockLostAlert);
    expect(screen.getByTestId("diagnostics")).toHaveTextContent("C:\\data");
    expect(screen.queryByTestId("diagnostics-empty")).not.toBeInTheDocument();
  });
});
