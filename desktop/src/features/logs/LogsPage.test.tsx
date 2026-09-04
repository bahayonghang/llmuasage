import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { describe, expect, it, vi } from "vitest";
import { COPY } from "../../app/i18n";
import { emitLogsNavigationIntent } from "../../app/secondary";
import { logRecord } from "../../test/fixtures";
import { LOGS_PAGE_SIZE, LogsPage } from "./LogsPage";

const invokeMock = vi.mocked(invoke);

function pageRecords(start: number, count: number) {
  return Array.from({ length: count }, (_, index) =>
    logRecord(`ev-${String(start + index).padStart(2, "0")}`, {
      event_at: `2026-09-01T00:${String(start + index).padStart(2, "0")}:00Z`,
    }),
  );
}

function logsRequest(call: unknown[] | undefined) {
  return (call?.[1] as { request: Record<string, unknown> }).request;
}

describe("LogsPage", () => {
  it("sends the session jump payload, page_size 20, cursor paging, and raw only on expand", async () => {
    const user = userEvent.setup();
    const first = pageRecords(1, 20);
    const second = pageRecords(21, 20);
    invokeMock.mockImplementation(async (command: string, args?: unknown) => {
      if (command !== "logs") {
        throw new Error(`unexpected ${command}`);
      }
      const request = (args as { request: Record<string, unknown> }).request;
      if (request.event_key) {
        return {
          records: [
            logRecord(String(request.event_key), {
              raw_json: `{"event_key":"${request.event_key}"}`,
            }),
          ],
          next_cursor: null,
        };
      }
      if (request.cursor === "cursor-2") {
        return { records: second, next_cursor: null };
      }
      return { records: first, next_cursor: "cursor-2" };
    });

    render(<LogsPage filter={{ range: "all", window: "all" }} copy={COPY.zh} />);
    await waitFor(() => expect(screen.getByTestId("logs-row-ev-01")).toBeInTheDocument());

    const firstCall = logsRequest(invokeMock.mock.calls[0]);
    expect(firstCall.page_size).toBe(LOGS_PAGE_SIZE);
    expect(firstCall.page_size).toBe(20);
    expect(firstCall.include_raw_json).toBe(false);
    expect(firstCall.session ?? null).toBeNull();
    expect(firstCall.event_key ?? null).toBeNull();

    await act(async () => {
      emitLogsNavigationIntent("sess-9");
    });
    await waitFor(() => {
      const last = logsRequest(invokeMock.mock.calls.at(-1));
      expect(last.session).toBe("sess-9");
      expect(last.page_size).toBe(20);
      expect(last.include_raw_json).toBe(false);
    });
    expect(screen.getByTestId("logs-session-filter")).toHaveTextContent("sess-9");

    await waitFor(() => expect(screen.getByTestId("logs-next")).toBeInTheDocument());
    const beforeNext = invokeMock.mock.calls.length;
    await user.click(screen.getByTestId("logs-next"));
    await waitFor(() => expect(screen.getByTestId("logs-row-ev-21")).toBeInTheDocument());
    const nextRequest = logsRequest(invokeMock.mock.calls[beforeNext]);
    expect(nextRequest.cursor).toBe("cursor-2");
    expect(nextRequest.page_size).toBe(20);
    expect(nextRequest.include_raw_json).toBe(false);
    expect(second[0]?.event_key).not.toBe(first[19]?.event_key);
    expect(screen.queryByTestId("logs-row-ev-01")).not.toBeInTheDocument();

    await user.click(screen.getByTestId("logs-row-ev-21"));
    await waitFor(() => expect(screen.getByTestId("logs-raw-ev-21")).toBeInTheDocument());
    const expandRequest = logsRequest(
      invokeMock.mock.calls.find(
        (call) => (call[1] as { request: { event_key?: string } }).request.event_key === "ev-21",
      ),
    );
    expect(expandRequest.event_key).toBe("ev-21");
    expect(expandRequest.include_raw_json).toBe(true);
    expect(expandRequest.page_size).toBe(20);
    expect(screen.getByTestId("logs-raw-ev-21")).toHaveTextContent('{"event_key":"ev-21"}');

    for (const call of invokeMock.mock.calls) {
      expect(logsRequest(call).page_size).toBe(20);
    }
  });
});
