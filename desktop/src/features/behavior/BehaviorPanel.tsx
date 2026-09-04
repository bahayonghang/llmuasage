import type { Copy } from "../../app/i18n";
import type {
  ActivityPayload,
  ModelComparePayload,
  OptimizePayload,
  SecondarySectionState,
  ToolsPayload,
} from "../../app/types";
import { shouldPaintSupportData, supportCopy, supportMessage } from "./support";

function SupportTag({
  level,
  copy,
}: {
  level?: string;
  copy: Copy;
}) {
  return (
    <span className="support-tag" data-level={level || "no_data"}>
      {supportCopy(copy, level)}
    </span>
  );
}

function SectionNote({
  testId,
  message,
}: {
  testId: string;
  message: string;
}) {
  return (
    <p className="muted" data-testid={testId}>
      {message}
    </p>
  );
}

export function BehaviorPanel({
  activity,
  tools,
  optimize,
  compare,
  copy,
}: {
  activity: SecondarySectionState<ActivityPayload>;
  tools: SecondarySectionState<ToolsPayload>;
  optimize: SecondarySectionState<OptimizePayload>;
  compare: SecondarySectionState<ModelComparePayload>;
  copy: Copy;
}) {
  const activitySupport = activity.payload?.support;
  const toolsSupport = tools.payload?.support;
  const optimizeSupport = optimize.payload?.support;
  const compareSupport = compare.payload?.support;
  const paintActivity = activity.status === "ready" && shouldPaintSupportData(activitySupport?.level);
  const paintTools = tools.status === "ready" && shouldPaintSupportData(toolsSupport?.level);
  const paintOptimize = optimize.status === "ready" && shouldPaintSupportData(optimizeSupport?.level);
  const paintCompare = compare.status === "ready" && shouldPaintSupportData(compareSupport?.level);

  return (
    <section id="behavior" className="block" data-testid="behavior-panel">
      <div className="section-eyebrow">{copy.navBehavior}</div>
      <h2 className="section-title">{copy.behaviorTitle}</h2>

      <div className="behavior-grid">
        <div data-testid="activity-panel" data-support={activitySupport?.level || activity.status}>
          <div className="panel-head">
            <h3>{copy.activityTitle}</h3>
            <SupportTag level={activitySupport?.level || (activity.status === "degraded" ? "degraded" : undefined)} copy={copy} />
          </div>
          {activity.status === "loading" ? (
            <SectionNote testId="activity-empty" message={copy.secondaryLoading} />
          ) : !paintActivity ? (
            <SectionNote
              testId="activity-empty"
              message={activity.error || supportMessage(activitySupport, copy)}
            />
          ) : (
            <table className="data-table">
              <thead>
                <tr>
                  <th>{copy.activityTitle}</th>
                  <th>{copy.metricTurns}</th>
                  <th>{copy.kpiCost}</th>
                </tr>
              </thead>
              <tbody>
                {(activity.payload?.breakdown ?? []).map((row) => (
                  <tr key={row.category}>
                    <td>{row.category}</td>
                    <td className="num">{row.turns}</td>
                    <td className="num">${row.estimated_cost_usd.toFixed(4)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>

        <div data-testid="tools-panel" data-support={toolsSupport?.level || tools.status}>
          <div className="panel-head">
            <h3>{copy.toolsTitle}</h3>
            <SupportTag level={toolsSupport?.level || (tools.status === "degraded" ? "degraded" : undefined)} copy={copy} />
          </div>
          {tools.status === "loading" ? (
            <SectionNote testId="tools-empty" message={copy.secondaryLoading} />
          ) : !paintTools ? (
            <SectionNote testId="tools-empty" message={tools.error || supportMessage(toolsSupport, copy)} />
          ) : (
            <table className="data-table">
              <thead>
                <tr>
                  <th>{copy.toolsTitle}</th>
                  <th>{copy.metricCalls}</th>
                  <th>{copy.kpiCost}</th>
                </tr>
              </thead>
              <tbody>
                {(tools.payload?.breakdown ?? []).map((row) => (
                  <tr key={`${row.tool_kind}-${row.tool_name}`}>
                    <td>{row.mcp_server ? `${row.mcp_server} / ${row.tool_name}` : row.tool_name}</td>
                    <td className="num">{row.calls}</td>
                    <td className="num">${row.estimated_cost_usd.toFixed(4)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>

        <div data-testid="optimize-panel" data-support={optimizeSupport?.level || optimize.status}>
          <div className="panel-head">
            <h3>{copy.optimizeTitle}</h3>
            <SupportTag
              level={optimizeSupport?.level || (optimize.status === "degraded" ? "degraded" : undefined)}
              copy={copy}
            />
          </div>
          {optimize.status === "loading" ? (
            <SectionNote testId="optimize-empty" message={copy.secondaryLoading} />
          ) : !paintOptimize ? (
            <SectionNote
              testId="optimize-empty"
              message={optimize.error || supportMessage(optimizeSupport, copy)}
            />
          ) : (
            <div>
              <p>
                {copy.optimizeScore}: {optimize.payload?.grade} ({optimize.payload?.score})
              </p>
              <p>
                {copy.optimizeSavings}: {optimize.payload?.estimated_savings_tokens}
              </p>
              {(optimize.payload?.findings ?? []).map((finding) => (
                <div key={finding.id} className="finding-card">
                  <strong>{finding.title}</strong>
                  <p className="muted">{finding.evidence}</p>
                </div>
              ))}
            </div>
          )}
        </div>

        <div
          data-testid="compare-panel"
          data-support={compareSupport?.level || compare.status}
        >
          <div className="panel-head">
            <h3>{copy.compareTitle}</h3>
            <SupportTag
              level={compareSupport?.level || (compare.status === "degraded" ? "degraded" : undefined)}
              copy={copy}
            />
          </div>
          {compare.status === "loading" ? (
            <SectionNote testId="compare-empty" message={copy.secondaryLoading} />
          ) : !paintCompare ? (
            <SectionNote
              testId="compare-empty"
              message={compare.error || compare.payload?.warning || supportMessage(compareSupport, copy)}
            />
          ) : (
            <table className="data-table" data-testid="compare-data">
              <thead>
                <tr>
                  <th>{copy.compareMetric}</th>
                  <th>{compare.payload?.model_a?.model || "A"}</th>
                  <th>{compare.payload?.model_b?.model || "B"}</th>
                </tr>
              </thead>
              <tbody>
                {(compare.payload?.metrics ?? []).map((row) => (
                  <tr key={row.id} data-testid="compare-metric-row">
                    <td>{row.label || row.id}</td>
                    <td className="num">{row.model_a_value}</td>
                    <td className="num">{row.model_b_value}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      </div>
    </section>
  );
}
