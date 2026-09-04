import { Fragment, useEffect, useRef, useState } from "react";
import type { Copy } from "../../app/i18n";
import { rangeToFilterDto } from "../../app/filters";
import { LOGS_NAVIGATE_EVENT } from "../../app/secondary";
import type {
  FilterDto,
  FilterState,
  LogRecord,
  LogsDto,
  LogsNavigationIntent,
  LogsPage,
} from "../../app/types";
import { allocateRequestId, invokeCommand, normalizeInvokeError } from "../../runtime/invoke";

export const LOGS_PAGE_SIZE = 20;

export function toLogsDto(args: {
  requestId: number;
  filter: FilterDto;
  cursor?: string | null;
  includeRawJson: boolean;
  session?: string | null;
  eventKey?: string | null;
}): LogsDto {
  return {
    request_id: args.requestId,
    filter: args.filter,
    page_size: LOGS_PAGE_SIZE,
    cursor: args.cursor ?? null,
    include_total: false,
    include_raw_json: args.includeRawJson,
    session: args.session ?? null,
    event_key: args.eventKey ?? null,
  };
}

export function LogsPage({ filter, copy }: { filter: FilterState; copy: Copy }) {
  const [session, setSession] = useState("");
  const [records, setRecords] = useState<LogRecord[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [rawMap, setRawMap] = useState<Record<string, string>>({});
  const generationRef = useRef(0);
  const sessionRef = useRef(session);
  sessionRef.current = session;
  const filterRef = useRef(filter);
  filterRef.current = filter;

  useEffect(() => {
    const onNavigate = (event: Event) => {
      const next = String((event as CustomEvent<LogsNavigationIntent>).detail?.session ?? "").trim();
      if (!next) {
        return;
      }
      setSession(next);
      window.location.hash = "#logs";
    };
    window.addEventListener(LOGS_NAVIGATE_EVENT, onNavigate);
    return () => window.removeEventListener(LOGS_NAVIGATE_EVENT, onNavigate);
  }, []);

  useEffect(() => {
    const generation = ++generationRef.current;
    const requestId = allocateRequestId();
    setLoading(true);
    setError(null);
    setRecords([]);
    setNextCursor(null);
    setExpanded(null);
    setRawMap({});
    void invokeCommand<LogsPage>("logs", {
      request: toLogsDto({
        requestId,
        filter: rangeToFilterDto(filter),
        includeRawJson: false,
        session: session || null,
      }),
    })
      .then((page) => {
        if (generation !== generationRef.current) {
          return;
        }
        setRecords(page.records ?? []);
        setNextCursor(page.next_cursor ?? null);
      })
      .catch((cause) => {
        if (generation !== generationRef.current) {
          return;
        }
        setError(normalizeInvokeError(cause).message);
      })
      .finally(() => {
        if (generation === generationRef.current) {
          setLoading(false);
        }
      });
  }, [filter, session]);

  async function loadNext(): Promise<void> {
    if (!nextCursor || loading) {
      return;
    }
    const generation = ++generationRef.current;
    const requestId = allocateRequestId();
    setLoading(true);
    setError(null);
    try {
      const page = await invokeCommand<LogsPage>("logs", {
        request: toLogsDto({
          requestId,
          filter: rangeToFilterDto(filterRef.current),
          cursor: nextCursor,
          includeRawJson: false,
          session: sessionRef.current || null,
        }),
      });
      if (generation !== generationRef.current) {
        return;
      }
      setRecords(page.records ?? []);
      setNextCursor(page.next_cursor ?? null);
      setExpanded(null);
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

  async function toggleRaw(eventKey: string): Promise<void> {
    if (expanded === eventKey) {
      setExpanded(null);
      return;
    }
    setExpanded(eventKey);
    if (rawMap[eventKey] != null) {
      return;
    }
    try {
      const page = await invokeCommand<LogsPage>("logs", {
        request: toLogsDto({
          requestId: allocateRequestId(),
          filter: rangeToFilterDto(filterRef.current),
          includeRawJson: true,
          session: sessionRef.current || null,
          eventKey,
        }),
      });
      const raw = page.records[0]?.raw_json || copy.logsRawUnavailable;
      setRawMap((current) => ({ ...current, [eventKey]: raw }));
    } catch (cause) {
      setRawMap((current) => ({
        ...current,
        [eventKey]: normalizeInvokeError(cause).message,
      }));
    }
  }

  return (
    <section id="logs" className="block" data-testid="logs-panel">
      <div className="section-eyebrow">{copy.navLogs}</div>
      <h2 className="section-title">{copy.logsTitle}</h2>
      {session ? (
        <div className="logs-session-filter" data-testid="logs-session-filter">
          <span>
            {copy.logsSession}: <strong>{session}</strong>
          </span>
          <button type="button" className="btn" data-testid="logs-clear-session" onClick={() => setSession("")}>
            {copy.logsClear}
          </button>
        </div>
      ) : null}
      {error ? (
        <p className="alert" data-testid="logs-error">
          {error}
        </p>
      ) : null}
      {loading && records.length === 0 ? (
        <p className="muted" data-testid="logs-loading">
          {copy.logsLoading}
        </p>
      ) : records.length === 0 ? (
        <p className="muted" data-testid="logs-empty">
          {copy.logsEmpty}
        </p>
      ) : (
        <div className="logs-table-wrap">
          <table className="data-table logs-table">
            <thead>
              <tr>
                <th>{copy.logsTime}</th>
                <th>{copy.logsSource}</th>
                <th>{copy.logsModel}</th>
                <th>{copy.logsSession}</th>
                <th className="num">{copy.logsTokens}</th>
                <th className="num">{copy.logsCost}</th>
                <th>{copy.logsProject}</th>
              </tr>
            </thead>
            <tbody>
              {records.map((row) => (
                <Fragment key={row.event_key}>
                  <tr
                    data-testid={`logs-row-${row.event_key}`}
                    data-event-key={row.event_key}
                    tabIndex={0}
                    aria-expanded={expanded === row.event_key}
                    onClick={() => void toggleRaw(row.event_key)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter" || event.key === " ") {
                        event.preventDefault();
                        void toggleRaw(row.event_key);
                      }
                    }}
                  >
                    <td>{row.event_at || "--"}</td>
                    <td>{row.source || "--"}</td>
                    <td>{row.model || "--"}</td>
                    <td>{row.session_label || row.session_id || "--"}</td>
                    <td className="num">{row.total_tokens}</td>
                    <td className="num">${Number(row.cost_usd || 0).toFixed(2)}</td>
                    <td>{row.project_label || "--"}</td>
                  </tr>
                  {expanded === row.event_key ? (
                    <tr className="log-raw-row" data-testid={`logs-raw-${row.event_key}`}>
                      <td colSpan={7}>
                        <pre>{rawMap[row.event_key] ?? copy.logsRawLoading}</pre>
                      </td>
                    </tr>
                  ) : null}
                </Fragment>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {nextCursor ? (
        <button
          type="button"
          className="btn"
          data-testid="logs-next"
          disabled={loading}
          onClick={() => void loadNext()}
        >
          {copy.logsMore}
        </button>
      ) : null}
    </section>
  );
}
