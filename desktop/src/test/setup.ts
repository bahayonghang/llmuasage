import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach, vi } from "vitest";
import { resetRequestIdCounter } from "../runtime/invoke";
import "../styles/layout.css";
import "../styles/tokens.css";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

afterEach(() => {
  cleanup();
  resetRequestIdCounter();
  vi.useRealTimers();
  vi.clearAllMocks();
  document.documentElement.removeAttribute("data-theme");
});
