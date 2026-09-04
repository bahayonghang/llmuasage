import { allocateRequestId, invokeCommand } from "../runtime/invoke";
import { rangeToFilterDto } from "./filters";
import type {
  ExplorerQueryState,
  FilterDto,
  FilterState,
  LogsNavigationIntent,
} from "./types";

export const SECONDARY_SECTIONS = Object.freeze([
  "activity",
  "tools",
  "optimize",
  "explorer",
  "compare",
  "home_overview",
  "heatmap",
  "trends_daily",
  "top_sessions",
  "hour_of_week",
] as const);

export type SecondarySection = (typeof SECONDARY_SECTIONS)[number];

export const SECONDARY_CONCURRENCY = 2;

export const LOGS_NAVIGATE_EVENT = "llmusage:logs-navigate";

export const DEFAULT_EXPLORER_QUERY: ExplorerQueryState = {
  granularity: "day",
  metric: "attributed_cost_usd",
  group_by: "source",
  session_id: "",
  tool_name: "",
  tool_kind: "",
  token_type: "",
  include_other: true,
  include_non_tool: true,
  limit: 8,
};

export type SecondaryLoader = () => Promise<unknown>;

export async function runLoadersWithConcurrency(
  loaders: Record<string, SecondaryLoader>,
  concurrency: number,
  onResult: (section: string, payload: unknown, error: unknown) => void | Promise<void>,
): Promise<void> {
  const entries = Object.entries(loaders);
  let nextIndex = 0;
  const worker = async () => {
    while (nextIndex < entries.length) {
      const current = entries[nextIndex];
      nextIndex += 1;
      if (!current) {
        return;
      }
      const [section, load] = current;
      let payload: unknown = null;
      let loadError: unknown = null;
      try {
        payload = await load();
      } catch (error) {
        loadError = error;
      }
      await onResult(section, payload, loadError);
    }
  };
  const workerCount = Math.min(Math.max(1, concurrency), entries.length);
  await Promise.all(Array.from({ length: workerCount }, () => worker()));
}

export function shouldAcceptSecondaryResult(
  loadGeneration: number,
  currentGeneration: number,
): boolean {
  return loadGeneration === currentGeneration;
}

export type HeatmapDrill = {
  date: string | null;
  previous: FilterState | null;
};

export function applyHeatmapDateClick(
  current: FilterState,
  drill: HeatmapDrill,
  date: string,
): { filter: FilterState; drill: HeatmapDrill } {
  if (drill.date === date) {
    return {
      filter: drill.previous ?? { range: "all", window: "all" },
      drill: { date: null, previous: null },
    };
  }
  return {
    filter: {
      ...current,
      range: "custom",
      since: date,
      until: date,
      window: current.window || "day",
    },
    drill: { date, previous: { ...current } },
  };
}

export function emitLogsNavigationIntent(session: string): LogsNavigationIntent {
  const intent: LogsNavigationIntent = { session };
  window.dispatchEvent(new CustomEvent(LOGS_NAVIGATE_EVENT, { detail: intent }));
  return intent;
}

export function shouldPaintSupportData(level: string | undefined): boolean {
  return level === "normalized" || level === "low_sample";
}

function nonempty(value?: string): string | undefined {
  const trimmed = value?.trim();
  return trimmed ? trimmed : undefined;
}

export function toExplorerDto(
  filter: FilterDto,
  requestId: number,
  query: ExplorerQueryState,
) {
  return {
    request_id: requestId,
    filter,
    granularity: query.granularity,
    metric: query.metric,
    group_by: query.group_by,
    session_id: nonempty(query.session_id),
    tool_name: nonempty(query.tool_name),
    tool_kind: nonempty(query.tool_kind),
    token_type: nonempty(query.token_type),
    include_other: query.include_other,
    include_non_tool: query.include_non_tool,
    limit: query.limit,
  };
}

export type SecondaryLoaderDeps = {
  filter: FilterDto;
  explorer: ExplorerQueryState;
  sessionsSort: string;
  invokeCommand?: typeof invokeCommand;
  allocateRequestId?: typeof allocateRequestId;
  onRequestId?: (id: number) => void;
  isCurrent?: () => boolean;
};

export function createSecondaryLoaders(
  deps: SecondaryLoaderDeps,
): Record<SecondarySection, SecondaryLoader> {
  const invoke = deps.invokeCommand ?? invokeCommand;
  const nextId = deps.allocateRequestId ?? allocateRequestId;

  const begin = (): number | null => {
    if (deps.isCurrent && !deps.isCurrent()) {
      return null;
    }
    const requestId = nextId();
    deps.onRequestId?.(requestId);
    return requestId;
  };

  const secondaryRequest = (command: SecondarySection): SecondaryLoader => async () => {
    const requestId = begin();
    if (requestId === null) {
      return;
    }
    return invoke(command, {
      request: { request_id: requestId, filter: deps.filter },
    });
  };

  return {
    activity: secondaryRequest("activity"),
    tools: secondaryRequest("tools"),
    optimize: secondaryRequest("optimize"),
    explorer: async () => {
      const requestId = begin();
      if (requestId === null) {
        return;
      }
      return invoke("explorer", {
        request: toExplorerDto(deps.filter, requestId, deps.explorer),
      });
    },
    compare: secondaryRequest("compare"),
    home_overview: secondaryRequest("home_overview"),
    heatmap: secondaryRequest("heatmap"),
    trends_daily: secondaryRequest("trends_daily"),
    top_sessions: async () => {
      const requestId = begin();
      if (requestId === null) {
        return;
      }
      return invoke("top_sessions", {
        request: {
          request_id: requestId,
          filter: deps.filter,
          sort: deps.sessionsSort,
          limit: 10,
        },
      });
    },
    hour_of_week: secondaryRequest("hour_of_week"),
  };
}

export type SecondaryLoadArgs = {
  generation: number;
  isCurrent: (generation: number) => boolean;
  filter: FilterState;
  explorer: ExplorerQueryState;
  sessionsSort: string;
  now?: Date;
  invokeCommand?: typeof invokeCommand;
  allocateRequestId?: typeof allocateRequestId;
  onRequestId?: (id: number) => void;
  onResult: (section: SecondarySection, payload: unknown, error: unknown) => void;
};

export async function loadSecondarySections(args: SecondaryLoadArgs): Promise<void> {
  const filter = rangeToFilterDto(args.filter, args.now ?? new Date());
  const loaders = createSecondaryLoaders({
    filter,
    explorer: args.explorer,
    sessionsSort: args.sessionsSort,
    invokeCommand: args.invokeCommand,
    allocateRequestId: args.allocateRequestId,
    onRequestId: args.onRequestId,
    isCurrent: () => args.isCurrent(args.generation),
  });
  await runLoadersWithConcurrency(loaders, SECONDARY_CONCURRENCY, (section, payload, error) => {
    if (!args.isCurrent(args.generation)) {
      return;
    }
    args.onResult(section as SecondarySection, payload, error);
  });
}
