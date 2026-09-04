import { invokeCommand } from "../runtime/invoke";
import { windowFromState } from "./filters";
import type {
  AutoRefreshMs,
  FilterDto,
  FilterState,
  Locale,
  PrefsDto,
  RangePreset,
  ThemeName,
} from "./types";

export const DEFAULT_PREFS: PrefsDto = {
  theme: "dark",
  locale: "zh",
  auto_refresh_ms: 0,
  filter: emptyFilterDto(),
  window: "all",
  range_preset: "all",
};

export const AUTO_REFRESH_OPTIONS: AutoRefreshMs[] = [0, 30_000, 60_000];

export function emptyFilterDto(): FilterDto {
  return {
    source: null,
    model: null,
    since: null,
    until: null,
    project_hash: null,
    host_id: null,
    timezone: null,
  };
}

export function isAutoRefreshMs(value: number): value is AutoRefreshMs {
  return value === 0 || value === 30_000 || value === 60_000;
}

function isRangePreset(value: string): value is RangePreset {
  return value === "1d" || value === "7d" || value === "30d" || value === "all" || value === "custom";
}

function nonempty(value?: string | null): string | undefined {
  const trimmed = value?.trim();
  return trimmed ? trimmed : undefined;
}

export function coercePrefs(raw: Partial<PrefsDto> | null | undefined): PrefsDto {
  const theme: ThemeName = raw?.theme === "light" ? "light" : "dark";
  const locale: Locale = raw?.locale === "en" ? "en" : "zh";
  const auto_refresh_ms: AutoRefreshMs = isAutoRefreshMs(Number(raw?.auto_refresh_ms))
    ? (Number(raw?.auto_refresh_ms) as AutoRefreshMs)
    : 0;
  const rangeCandidate = raw?.range_preset ?? "all";
  const range_preset = isRangePreset(rangeCandidate) ? rangeCandidate : "all";
  const window =
    raw?.window === "day" || raw?.window === "week" || raw?.window === "month" || raw?.window === "all"
      ? raw.window
      : "all";
  const incoming = raw?.filter ?? {};
  return {
    theme,
    locale,
    auto_refresh_ms,
    filter: {
      source: incoming.source ?? null,
      model: incoming.model ?? null,
      since: incoming.since ?? null,
      until: incoming.until ?? null,
      project_hash: incoming.project_hash ?? null,
      host_id: incoming.host_id ?? null,
      timezone: incoming.timezone ?? null,
    },
    window,
    range_preset,
  };
}

export function filterFromPrefs(prefs: PrefsDto): FilterState {
  const range = isRangePreset(prefs.range_preset) ? prefs.range_preset : "all";
  const state: FilterState = {
    range,
    window: prefs.window,
    source: nonempty(prefs.filter.source),
    model: nonempty(prefs.filter.model),
    project_hash: nonempty(prefs.filter.project_hash),
    host_id: nonempty(prefs.filter.host_id),
  };
  if (range === "custom") {
    state.since = nonempty(prefs.filter.since);
    state.until = nonempty(prefs.filter.until);
  }
  return state;
}

export function toPrefsDto(
  theme: ThemeName,
  locale: Locale,
  autoRefreshMs: AutoRefreshMs,
  filter: FilterState,
): PrefsDto {
  return {
    theme,
    locale,
    auto_refresh_ms: autoRefreshMs,
    filter: {
      source: filter.source ?? null,
      model: filter.model ?? null,
      since: filter.range === "custom" ? filter.since ?? null : null,
      until: filter.range === "custom" ? filter.until ?? null : null,
      project_hash: filter.project_hash ?? null,
      host_id: filter.host_id ?? null,
      timezone: null,
    },
    window: windowFromState(filter),
    range_preset: filter.range,
  };
}

export async function loadPrefs(): Promise<PrefsDto> {
  try {
    return coercePrefs(await invokeCommand<PrefsDto>("load_prefs"));
  } catch {
    return { ...DEFAULT_PREFS, filter: emptyFilterDto() };
  }
}

export async function savePrefs(prefs: PrefsDto): Promise<PrefsDto> {
  return invokeCommand<PrefsDto>("save_prefs", { prefs: coercePrefs(prefs) });
}

export type AutoRefreshController = {
  getInterval(): AutoRefreshMs;
  setInterval(ms: AutoRefreshMs): void;
  stop(): void;
};

export function createAutoRefreshController(
  initial: AutoRefreshMs,
  onTick: () => void,
): AutoRefreshController {
  let current: AutoRefreshMs = isAutoRefreshMs(initial) ? initial : 0;
  let timer: ReturnType<typeof setInterval> | null = null;

  const apply = () => {
    if (timer != null) {
      clearInterval(timer);
      timer = null;
    }
    if (current !== 0) {
      timer = setInterval(onTick, current);
    }
  };

  apply();

  return {
    getInterval: () => current,
    setInterval(ms) {
      current = isAutoRefreshMs(ms) ? ms : 0;
      apply();
    },
    stop() {
      if (timer != null) {
        clearInterval(timer);
        timer = null;
      }
    },
  };
}
