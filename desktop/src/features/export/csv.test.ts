import { describe, expect, it } from "vitest";
import { buildAnalyticsCsv, escapeCsvCell } from "./csv";

describe("escapeCsvCell", () => {
  it("neutralizes formula prefixes including tab/CR/LF", () => {
    for (const value of ["=cmd()", "+1", "-1", "@x", "\tformula", "\rformula", "\nformula"]) {
      expect(escapeCsvCell(value).replace(/^"/, "").startsWith("'"), value).toBe(true);
    }
    expect(escapeCsvCell('a,"b"\nline')).toBe('"a,""b""\nline"');
  });
});

describe("buildAnalyticsCsv", () => {
  it("emits UTF-8 BOM, six section titles, and formula neutralization", () => {
    const data = {
      home_overview: {
        summary: {
          total_sessions: 1,
          total_requests: 2,
          total_tokens: 3,
          total_cost_usd: 4,
          active_days: 5,
          cache_efficiency: 0.25,
          platforms: 9,
        },
      },
      projects: [{ project_label: "=cmd()", total_tokens: 3, total_cost_usd: 0 }],
      models: [{ model: "+1", total_tokens: 1, cost_with_cache_usd: 0 }],
      sources: [{ source: "-1", total_tokens: 1 }],
      trends_daily: [{ date: "2026-09-01", total_tokens: 3, cost_with_cache_usd: 0.1 }],
      top_sessions: [{ session_label: "@x", total_tokens: 2, active_minutes: 1, cost_usd: 0.2 }],
    };
    const zh = buildAnalyticsCsv(data, "zh");
    const en = buildAnalyticsCsv(data, "en");
    expect(zh.charCodeAt(0)).toBe(0xfeff);
    const bytes = new TextEncoder().encode(zh);
    expect([...bytes.slice(0, 3)]).toEqual([0xef, 0xbb, 0xbf]);
    expect(zh).toMatch(/汇总\r\n指标,值/);
    expect(zh).toContain("每日 Token 用量");
    expect(zh).toContain("项目");
    expect(zh).toContain("模型");
    expect(zh).toContain("来源");
    expect(zh).toContain("高用量会话");
    expect(zh.split("\r\n\r\n")[0].split("\r\n").length).toBe(8);
    expect(zh).not.toMatch(/platforms/);
    expect(en).toMatch(/Summary\r\nMetric,Value/);
    expect(en).toContain("Daily token usage");
    expect(en).toContain("Projects");
    expect(en).toContain("Models");
    expect(en).toContain("Sources");
    expect(en).toContain("Highest-usage sessions");
    expect(zh).toMatch(/'=cmd\(\)/);
    expect(zh).toMatch(/'\+1/);
    expect(zh).toMatch(/'-1/);
    expect(zh).toMatch(/'@x/);
    expect(zh.split("\r\n\r\n").length).toBeGreaterThanOrEqual(6);
  });
});
