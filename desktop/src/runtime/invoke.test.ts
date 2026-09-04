import { invoke } from "@tauri-apps/api/core";
import { describe, expect, it, vi } from "vitest";
import invokeSource from "./invoke.ts?raw";
import { allocateRequestId, invokeCommand, normalizeInvokeError } from "./invoke";

const invokeMock = vi.mocked(invoke);

const productionSources = import.meta.glob("../**/*.{ts,tsx}", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

describe("invokeCommand", () => {
  it("is the unique invoke wrapper and never uses /api/ paths", async () => {
    const source = invokeSource;
    expect(source).not.toMatch(/["'`]\/api\//);
    expect(source).toContain("invokeCommand");

    for (const [path, contents] of Object.entries(productionSources)) {
      if (path.includes(".test.") || path.includes("/test/")) {
        continue;
      }
      expect(contents, path).not.toMatch(/["'`]\/api\//);
      if (!path.endsWith("invoke.ts")) {
        expect(contents, path).not.toMatch(/from ["']@tauri-apps\/api/);
      }
      if (path.endsWith("load-state.ts")) {
        expect(contents, path).not.toMatch(/\bhome_overview\b/);
      }
    }

    invokeMock.mockResolvedValueOnce({ ok: true });
    const requestId = allocateRequestId();
    await invokeCommand("dashboard_interactive", {
      request: { request_id: requestId, filter: {}, window: "all" },
    });
    expect(invokeMock).toHaveBeenCalledWith("dashboard_interactive", {
      request: { request_id: requestId, filter: {}, window: "all" },
    });
    expect(String(invokeMock.mock.calls[0]?.[0])).not.toContain("/api/");
  });

  it("parses lock_busy holder from objects and Display strings", () => {
    expect(
      normalizeInvokeError({
        code: "lock_busy",
        message: "worker lock busy",
        holder: "cli:1@t",
      }),
    ).toEqual({ code: "lock_busy", message: "worker lock busy", holder: "cli:1@t" });
    expect(normalizeInvokeError("lock_busy: worker lock busy (cli:1@t)")).toEqual({
      code: "lock_busy",
      message: "worker lock busy",
      holder: "cli:1@t",
    });
    expect(normalizeInvokeError({ error: { code: "lock_lost", message: "lost" } })).toEqual({
      code: "lock_lost",
      message: "lost",
    });
  });
});
