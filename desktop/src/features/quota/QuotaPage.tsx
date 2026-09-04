import { useEffect, useRef, useState } from "react";
import type { Copy } from "../../app/i18n";
import type { QuotaResponse, UsageOutput } from "../../app/types";
import { invokeCommand, normalizeInvokeError } from "../../runtime/invoke";

export const HIDDEN_EMAIL = "[hidden email]";

export function displayQuotaEmail(email: string | null | undefined, hide: boolean): string {
  const trimmed = email?.trim();
  if (!trimmed) {
    return "-";
  }
  return hide ? HIDDEN_EMAIL : trimmed;
}

export function QuotaPage({ copy }: { copy: Copy }) {
  const [response, setResponse] = useState<QuotaResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [hideEmail, setHideEmail] = useState(true);
  const generationRef = useRef(0);

  async function load(bypassCache: boolean): Promise<void> {
    const generation = ++generationRef.current;
    setLoading(true);
    setError(null);
    try {
      const next = await invokeCommand<QuotaResponse>("fetch_quota", {
        bypass_cache: bypassCache,
      });
      if (generation !== generationRef.current) {
        return;
      }
      setResponse(next);
    } catch (cause) {
      if (generation !== generationRef.current) {
        return;
      }
      setError(normalizeInvokeError(cause).message);
    } finally {
      if (generation === generationRef.current) {
        setLoading(false);
      }
    }
  }

  useEffect(() => {
    void load(false);
  }, []);

  const outputs = response?.report.outputs ?? [];
  const diagnostics = response?.report.diagnostics ?? [];
  const empty = !loading && !error && outputs.length === 0;

  return (
    <section id="quota" className="block" data-testid="quota-panel">
      <div className="ready-widget-head">
        <div>
          <div className="section-eyebrow">{copy.navQuota}</div>
          <h2 className="section-title">{copy.quotaTitle}</h2>
        </div>
        <div className="seg">
          <button
            type="button"
            data-testid="quota-email-toggle"
            onClick={() => setHideEmail((current) => !current)}
          >
            {hideEmail ? copy.quotaShowEmail : copy.quotaHideEmail}
          </button>
          <button
            type="button"
            className="btn"
            data-testid="quota-refresh"
            disabled={loading}
            onClick={() => void load(true)}
          >
            {copy.quotaRefresh}
          </button>
        </div>
      </div>
      {response ? (
        <p data-testid="quota-cache-hit" data-cache-hit={String(response.cache_hit)}>
          {copy.quotaCacheHit}: {String(response.cache_hit)}
        </p>
      ) : null}
      {loading ? (
        <p className="muted" data-testid="quota-loading">
          {copy.quotaLoading}
        </p>
      ) : null}
      {error ? (
        <p className="alert" data-testid="quota-error">
          {copy.quotaFailed}: {error}
        </p>
      ) : null}
      {empty ? (
        <p className="muted" data-testid="quota-empty">
          {copy.quotaEmpty}
        </p>
      ) : null}
      {outputs.length > 0 ? (
        <div className="quota-outputs" data-testid="quota-outputs">
          {outputs.map((output) => (
            <QuotaOutputCard key={output.provider} output={output} hideEmail={hideEmail} copy={copy} />
          ))}
        </div>
      ) : null}
      {diagnostics.length > 0 ? (
        <div data-testid="quota-diagnostics">
          <h3>{copy.diagnosticsTitle}</h3>
          <ul>
            {diagnostics.map((row) => (
              <li key={`${row.provider}-${row.message}`}>
                {row.provider}: {row.message}
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </section>
  );
}

function QuotaOutputCard({
  output,
  hideEmail,
  copy,
}: {
  output: UsageOutput;
  hideEmail: boolean;
  copy: Copy;
}) {
  const slug = output.provider.replaceAll(" ", "-").toLowerCase();
  return (
    <article className="quota-card" data-testid={`quota-output-${slug}`} data-provider={output.provider}>
      <h3>{output.provider}</h3>
      <p>
        {copy.quotaPlan}: {output.plan || "-"}
      </p>
      <p data-testid={`quota-email-${slug}`}>{displayQuotaEmail(output.email, hideEmail)}</p>
      <ul>
        {output.metrics.map((metric) => (
          <li key={metric.label}>
            {metric.label}: {metric.remaining_label || `${metric.remaining_percent.toFixed(0)}%`}
          </li>
        ))}
      </ul>
    </article>
  );
}
