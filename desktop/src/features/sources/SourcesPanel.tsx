import type { Copy } from "../../app/i18n";
import type { SourceBreakdown } from "../../app/types";

export function SourcesPanel({ sources, copy }: { sources: SourceBreakdown[]; copy: Copy }) {
  return (
    <section id="sources" className="block" data-testid="sources-panel">
      <div className="section-eyebrow">{copy.navSources}</div>
      <h2 className="section-title">{copy.sourcesTitle}</h2>
      {sources.length === 0 ? (
        <p className="muted">{copy.emptyRows}</p>
      ) : (
        <div className="row-list">
          {sources.map((row) => (
            <div className="source-row" key={row.source}>
              <div>{row.source}</div>
              <div className="muted">{row.last_event_at ?? "--"}</div>
              <div className="num">{row.total_tokens}</div>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
