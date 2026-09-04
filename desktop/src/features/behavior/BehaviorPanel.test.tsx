import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { COPY } from "../../app/i18n";
import { insufficientCompare } from "../../test/fixtures";
import { BehaviorPanel } from "./BehaviorPanel";

const loading = { status: "loading" as const, payload: null };

describe("BehaviorPanel", () => {
  it("does not paint compare zeros when support is insufficient_models", () => {
    render(
      <BehaviorPanel
        activity={loading}
        tools={loading}
        optimize={loading}
        compare={{ status: "ready", payload: insufficientCompare() }}
        copy={COPY.zh}
      />,
    );
    const panel = screen.getByTestId("compare-panel");
    expect(panel).toHaveAttribute("data-support", "insufficient_models");
    expect(screen.queryByTestId("compare-data")).not.toBeInTheDocument();
    expect(screen.queryByTestId("compare-metric-row")).not.toBeInTheDocument();
    expect(screen.getByTestId("compare-empty")).toBeInTheDocument();
    expect(panel.textContent).not.toMatch(/\b0\b/);
  });
});
