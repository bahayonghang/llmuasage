import type { Copy } from "../../app/i18n";
import { shouldPaintSupportData } from "../../app/secondary";
import type { ExplorerPayload, ExplorerQueryState, SecondarySectionStatus } from "../../app/types";
import { supportCopy } from "../behavior/support";

const METRICS = [
  ["attributed_cost_usd", "metricCost"],
  ["calls", "metricCalls"],
  ["turns", "metricTurns"],
  ["sessions", "metricSessions"],
  ["total_tokens", "metricTokens"],
] as const;

const GROUPS = [
  ["source", "groupSource"],
  ["model", "groupModel"],
  ["project", "groupProject"],
  ["session", "groupSession"],
  ["tool", "groupTool"],
  ["tool_kind", "groupToolKind"],
  ["is_tool", "groupIsTool"],
  ["token_type", "groupTokenType"],
] as const;

const GRANULARITIES = [
  ["total", "granularityTotal"],
  ["day", "granularityDay"],
  ["week", "granularityWeek"],
  ["month", "granularityMonth"],
] as const;

export function ExplorerPanel({
  query,
  status,
  payload,
  reason,
  copy,
  onChange,
}: {
  query: ExplorerQueryState;
  status: SecondarySectionStatus;
  payload: ExplorerPayload | null;
  reason?: string;
  copy: Copy;
  onChange: (next: ExplorerQueryState) => void;
}) {
  const support = payload?.support;
  const unsupported = support?.level === "unsupported";
  const disabled = unsupported;
  const paint = status === "ready" && shouldPaintSupportData(support?.level);
  const shownReason = reason || support?.reason || payload?.warning || "";

  function patch(partial: Partial<ExplorerQueryState>) {
    onChange({ ...query, ...partial });
  }

  return (
    <section id="explorer" className="block" data-testid="explorer-panel" data-state={status}>
      <div className="section-head">
        <div>
          <div className="section-eyebrow">{copy.navExplorer}</div>
          <h2 className="section-title">{copy.explorerTitle}</h2>
        </div>
        <span className="support-tag" data-testid="explorer-support" data-level={support?.level || status}>
          {supportCopy(copy, support?.level || (status === "degraded" ? "degraded" : undefined))}
        </span>
      </div>

      <fieldset className="explorer-controls" id="explorer-controls" disabled={disabled}>
        <div className="filter-group">
          <label htmlFor="explorer-metric">{copy.explorerMetric}</label>
          <select
            id="explorer-metric"
            data-testid="explorer-metric"
            value={query.metric}
            onChange={(event) => patch({ metric: event.target.value })}
          >
            {METRICS.map(([value, key]) => (
              <option key={value} value={value}>
                {copy[key]}
              </option>
            ))}
          </select>
        </div>
        <div className="filter-group">
          <label htmlFor="explorer-group-by">{copy.explorerGroupBy}</label>
          <select
            id="explorer-group-by"
            data-testid="explorer-group-by"
            value={query.group_by}
            onChange={(event) => patch({ group_by: event.target.value })}
          >
            {GROUPS.map(([value, key]) => (
              <option key={value} value={value}>
                {copy[key]}
              </option>
            ))}
          </select>
        </div>
        <div className="filter-group">
          <label htmlFor="explorer-granularity">{copy.explorerGranularity}</label>
          <select
            id="explorer-granularity"
            data-testid="explorer-granularity"
            value={query.granularity}
            onChange={(event) => patch({ granularity: event.target.value })}
          >
            {GRANULARITIES.map(([value, key]) => (
              <option key={value} value={value}>
                {copy[key]}
              </option>
            ))}
          </select>
        </div>
        <div className="filter-group">
          <label htmlFor="explorer-limit">{copy.explorerLimit}</label>
          <input
            id="explorer-limit"
            data-testid="explorer-limit"
            type="number"
            min={1}
            max={50}
            value={query.limit}
            onChange={(event) => patch({ limit: Number(event.target.value) || 8 })}
          />
        </div>
        <div className="filter-group">
          <label htmlFor="explorer-session">{copy.explorerSession}</label>
          <input
            id="explorer-session"
            data-testid="explorer-session-id"
            value={query.session_id}
            onChange={(event) => patch({ session_id: event.target.value })}
          />
        </div>
        <div className="filter-group">
          <label htmlFor="explorer-tool-name">{copy.explorerToolName}</label>
          <input
            id="explorer-tool-name"
            data-testid="explorer-tool-name"
            value={query.tool_name}
            onChange={(event) => patch({ tool_name: event.target.value })}
          />
        </div>
        <div className="filter-group">
          <label htmlFor="explorer-tool-kind">{copy.explorerToolKind}</label>
          <select
            id="explorer-tool-kind"
            data-testid="explorer-tool-kind"
            value={query.tool_kind}
            onChange={(event) => patch({ tool_kind: event.target.value })}
          >
            <option value="">{copy.explorerAll}</option>
            <option value="read">read</option>
            <option value="edit">edit</option>
            <option value="shell">shell</option>
            <option value="mcp">mcp</option>
            <option value="agent">agent</option>
            <option value="(non-tool)">(non-tool)</option>
          </select>
        </div>
        <div className="filter-group">
          <label htmlFor="explorer-token-type">{copy.explorerTokenType}</label>
          <select
            id="explorer-token-type"
            data-testid="explorer-token-type"
            value={query.token_type}
            onChange={(event) => patch({ token_type: event.target.value })}
          >
            <option value="">{copy.explorerAll}</option>
            <option value="input">input</option>
            <option value="cache_read">cache_read</option>
            <option value="cache_creation">cache_creation</option>
            <option value="output">output</option>
            <option value="reasoning_output">reasoning_output</option>
          </select>
        </div>
        <label className="explorer-check">
          <input
            data-testid="explorer-include-other"
            type="checkbox"
            checked={query.include_other}
            onChange={(event) => patch({ include_other: event.target.checked })}
          />
          {copy.explorerIncludeOther}
        </label>
        <label className="explorer-check">
          <input
            data-testid="explorer-include-non-tool"
            type="checkbox"
            checked={query.include_non_tool}
            onChange={(event) => patch({ include_non_tool: event.target.checked })}
          />
          {copy.explorerIncludeNonTool}
        </label>
      </fieldset>

      {unsupported || status === "degraded" || shownReason ? (
        <p className="muted" data-testid="explorer-reason">
          {shownReason || copy.emptyRows}
        </p>
      ) : null}

      {paint && (payload?.rows.length ?? 0) > 0 ? (
        <table className="data-table" data-testid="explorer-rows">
          <thead>
            <tr>
              <th>{copy.explorerGroupBy}</th>
              <th>{copy.explorerMetric}</th>
            </tr>
          </thead>
          <tbody>
            {payload?.rows.map((row) => (
              <tr key={row.key}>
                <td>{row.label || row.key}</td>
                <td className="num">{row.value}</td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : null}
    </section>
  );
}
