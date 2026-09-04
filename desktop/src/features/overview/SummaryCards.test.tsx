import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { COPY } from "../../app/i18n";
import { emptyHomeOverview } from "../../test/fixtures";
import { SummaryCards } from "./SummaryCards";

describe("SummaryCards", () => {
  it("renders six values from home_overview.summary", () => {
    render(
      <SummaryCards
        status="ready"
        summary={emptyHomeOverview().summary}
        copy={COPY.zh}
      />,
    );
    expect(screen.getByTestId("summary-cards")).toHaveAttribute("data-state", "ready");
    expect(screen.getByTestId("summary-card-sessions")).toHaveTextContent("4");
    expect(screen.getByTestId("summary-card-requests")).toHaveTextContent("12");
    expect(screen.getByTestId("summary-card-tokens")).toHaveTextContent("100");
    expect(screen.getByTestId("summary-card-active_days")).toHaveTextContent("3");
    expect(screen.getByTestId("summary-card-cache_efficiency")).toHaveTextContent("25.0%");
  });

  it("renders six degraded cards without painting zeros as data", () => {
    render(
      <SummaryCards status="degraded" summary={null} reason="timeout" copy={COPY.zh} />,
    );
    expect(screen.getByTestId("summary-cards")).toHaveAttribute("data-state", "degraded");
    expect(screen.getByTestId("summary-card-sessions")).toHaveTextContent("—");
    expect(screen.getByTestId("summary-card-sessions")).not.toHaveTextContent("0");
    expect(screen.getByTestId("summary-cards-reason")).toHaveTextContent("timeout");
  });
});
