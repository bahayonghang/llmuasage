import { render, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { Shell } from "../app/shell";
import { emptyInteractiveSnapshot, runtimeInfo } from "../test/fixtures";

const invokeMock = vi.mocked(invoke);

function computedToken(name: string): string {
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  if (value.startsWith("var(")) {
    const inner = value.slice(4, -1).trim();
    return getComputedStyle(document.documentElement).getPropertyValue(inner).trim();
  }
  return value;
}

describe("theme tokens", () => {
  beforeEach(() => {
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
      return null;
    });
  });

  it("matches base.css --bg-primary and --accent for light and dark", async () => {
    const user = userEvent.setup();
    document.documentElement.setAttribute("data-theme", "light");
    expect(computedToken("--bg-primary")).toBe("#f5f6f8");
    expect(computedToken("--accent")).toBe("#2563eb");
    expect(computedToken("--accent-blue")).toBe("#2563eb");

    document.documentElement.setAttribute("data-theme", "dark");
    expect(computedToken("--bg-primary")).toBe("#0d0d12");
    expect(computedToken("--accent")).toBe("#60a5fa");
    expect(computedToken("--accent-blue")).toBe("#60a5fa");

    render(<Shell />);
    await waitFor(() => expect(document.documentElement.getAttribute("data-theme")).toBe("dark"));
    expect(computedToken("--bg-primary")).toBe("#0d0d12");
    expect(computedToken("--accent")).toBe("#60a5fa");

    await user.click(document.querySelector('[data-testid="theme-toggle"]') as HTMLButtonElement);
    await waitFor(() => expect(document.documentElement.getAttribute("data-theme")).toBe("light"));
    expect(computedToken("--bg-primary")).toBe("#f5f6f8");
    expect(computedToken("--accent")).toBe("#2563eb");
  });
});
