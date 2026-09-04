import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { COPY } from "../../app/i18n";
import { DEFAULT_EXPLORER_QUERY } from "../../app/secondary";
import { emptyExplorer } from "../../test/fixtures";
import { ExplorerPanel } from "./ExplorerPanel";

describe("ExplorerPanel", () => {
  it("disables controls and shows the reason when explorer is unsupported", () => {
    const onChange = vi.fn();
    render(
      <ExplorerPanel
        query={DEFAULT_EXPLORER_QUERY}
        status="ready"
        payload={emptyExplorer({
          support: {
            supported: false,
            level: "unsupported",
            reason: "token_type filters only support the Token usage metric.",
            strategy: "none",
          },
          warning: "token_type filters only support the Token usage metric.",
        })}
        copy={COPY.en}
        onChange={onChange}
      />,
    );
    expect(screen.getByTestId("explorer-metric")).toBeDisabled();
    expect(screen.getByTestId("explorer-group-by")).toBeDisabled();
    expect(screen.getByTestId("explorer-include-non-tool")).toBeDisabled();
    expect(screen.getByTestId("explorer-reason")).toHaveTextContent(
      "token_type filters only support the Token usage metric.",
    );
  });
});
