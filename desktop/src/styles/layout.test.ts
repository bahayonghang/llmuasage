import { describe, expect, it } from "vitest";
import layoutCss from "./layout.css?raw";

describe("layout.css", () => {
  it("uses a 248px sidebar and collapses to a horizontal row at 720px without page overflow", () => {
    expect(layoutCss).toMatch(/grid-template-columns:\s*248px minmax\(0, 1fr\)/);
    expect(layoutCss).toMatch(/width:\s*248px/);
    expect(layoutCss).toMatch(/\.app \{[\s\S]*?overflow-x:\s*hidden/);
    const narrow = layoutCss.split("@media (max-width: 720px)")[1] ?? "";
    expect(narrow).toMatch(/aside\.sidebar \{[\s\S]*?flex-direction:\s*row/);
    expect(narrow).toMatch(/\.app \{[\s\S]*?overflow-x:\s*hidden/);
  });
});
