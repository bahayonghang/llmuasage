import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { describe, expect, it, vi } from "vitest";
import { COPY } from "../../app/i18n";
import { emptyQuota } from "../../test/fixtures";
import { displayQuotaEmail, HIDDEN_EMAIL, QuotaPage } from "./QuotaPage";

const invokeMock = vi.mocked(invoke);

const outputReport = {
  outputs: [
    {
      provider: "Codex",
      plan: "plus",
      email: "user@example.com",
      metrics: [{ label: "Session", used_percent: 40, remaining_percent: 60, remaining_label: "60% left" }],
    },
  ],
  diagnostics: [{ provider: "Claude", message: "HTTP 429", severity: "error" }],
};

describe("displayQuotaEmail", () => {
  it("defaults to [hidden email]", () => {
    expect(displayQuotaEmail("user@example.com", true)).toBe(HIDDEN_EMAIL);
    expect(displayQuotaEmail("user@example.com", false)).toBe("user@example.com");
    expect(displayQuotaEmail(null, true)).toBe("-");
  });
});

describe("QuotaPage", () => {
  it("shows empty state when there are no credentials/outputs", async () => {
    invokeMock.mockResolvedValue(emptyQuota());
    render(<QuotaPage copy={COPY.zh} />);
    await waitFor(() => expect(screen.getByTestId("quota-empty")).toBeInTheDocument());
    expect(screen.queryByTestId("quota-outputs")).not.toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("fetch_quota", { bypass_cache: false });
  });

  it("shows outputs, hidden email, diagnostics, cache_hit, then refresh bypasses cache", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string, args?: unknown) => {
      if (command !== "fetch_quota") {
        throw new Error(`unexpected ${command}`);
      }
      const bypass = Boolean((args as { bypass_cache?: boolean } | undefined)?.bypass_cache);
      return {
        cache_hit: !bypass,
        report: outputReport,
      };
    });
    render(<QuotaPage copy={COPY.en} />);
    await waitFor(() => expect(screen.getByTestId("quota-output-codex")).toBeInTheDocument());
    expect(screen.getByTestId("quota-cache-hit")).toHaveAttribute("data-cache-hit", "true");
    expect(screen.getByTestId("quota-email-codex")).toHaveTextContent(HIDDEN_EMAIL);
    expect(screen.getByTestId("quota-diagnostics")).toHaveTextContent("HTTP 429");
    expect(screen.getByTestId("quota-outputs")).toHaveTextContent("plus");
    await user.click(screen.getByTestId("quota-refresh"));
    await waitFor(() => expect(screen.getByTestId("quota-cache-hit")).toHaveAttribute("data-cache-hit", "false"));
    expect(invokeMock).toHaveBeenCalledWith("fetch_quota", { bypass_cache: true });
  });
});
