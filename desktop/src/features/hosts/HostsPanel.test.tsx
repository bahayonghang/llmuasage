import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { COPY } from "../../app/i18n";
import { hostRow } from "../../test/fixtures";
import { HostsPanel } from "./HostsPanel";

describe("HostsPanel", () => {
  it("does not render when hosts length is 0 or 1", () => {
    const { rerender } = render(
      <HostsPanel hosts={[]} copy={COPY.zh} onHostClick={() => undefined} />,
    );
    expect(screen.queryByTestId("hosts-panel")).not.toBeInTheDocument();

    rerender(
      <HostsPanel hosts={[hostRow("only")]} copy={COPY.zh} onHostClick={() => undefined} />,
    );
    expect(screen.queryByTestId("hosts-panel")).not.toBeInTheDocument();
  });

  it("renders and is clickable when hosts length is at least 2", async () => {
    const onHostClick = vi.fn();
    const user = userEvent.setup();
    render(
      <HostsPanel
        hosts={[hostRow("a", "Alpha"), hostRow("b", "Beta")]}
        copy={COPY.zh}
        onHostClick={onHostClick}
      />,
    );
    expect(screen.getByTestId("hosts-panel")).toBeInTheDocument();
    await user.click(screen.getByTestId("host-row-a"));
    expect(onHostClick).toHaveBeenCalledWith("a");
  });
});
