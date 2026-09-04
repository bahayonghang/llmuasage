import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { COPY } from "../../app/i18n";
import { emitLogsNavigationIntent, LOGS_NAVIGATE_EVENT } from "../../app/secondary";
import { TopSessionsPanel } from "./TopSessionsPanel";

describe("TopSessionsPanel", () => {
  it("emits a logs navigation intent with the session key", async () => {
    const user = userEvent.setup();
    const onSessionClick = vi.fn((session: string) => emitLogsNavigationIntent(session));
    const seen: unknown[] = [];
    const handler = (event: Event) => {
      seen.push((event as CustomEvent).detail);
    };
    window.addEventListener(LOGS_NAVIGATE_EVENT, handler);
    render(
      <TopSessionsPanel
        status="ready"
        sort="tokens"
        copy={COPY.zh}
        onSortChange={() => undefined}
        onSessionClick={onSessionClick}
        rows={[
          {
            session_id: "sess-9",
            session_label: "nine",
            project_label: "p",
            source: "codex",
            first_event_at: "t0",
            last_event_at: "t1",
            total_tokens: 8,
            output_tokens: 1,
            cost_usd: 0.1,
            span_minutes: 10,
            active_minutes: 4,
            event_count: 2,
          },
        ]}
      />,
    );
    await user.click(screen.getByTestId("session-row-sess-9"));
    expect(onSessionClick).toHaveBeenCalledWith("sess-9");
    expect(seen).toEqual([{ session: "sess-9" }]);
    window.removeEventListener(LOGS_NAVIGATE_EVENT, handler);
  });
});
