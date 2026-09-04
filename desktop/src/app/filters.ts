import type { FilterDto, FilterState, RangePreset, SyncStartDto } from "./types";

export const RANGE_TO_TREND_WINDOW: Record<Exclude<RangePreset, "custom">, string> = {
  "1d": "day",
  "7d": "week",
  "30d": "month",
  all: "all",
};

export function formatLocalYmd(date: Date): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function addLocalDays(date: Date, days: number): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate() + days);
}

function resolvedTimezone(): string | undefined {
  return Intl.DateTimeFormat().resolvedOptions().timeZone || undefined;
}

function assignIfPresent(target: FilterDto, key: keyof FilterDto, value?: string): void {
  if (value) {
    target[key] = value;
  }
}

export function rangeToFilterDto(state: FilterState, now = new Date()): FilterDto {
  const dto: FilterDto = {};
  const timezone = resolvedTimezone();
  assignIfPresent(dto, "timezone", timezone);
  assignIfPresent(dto, "source", state.source);
  assignIfPresent(dto, "model", state.model);
  assignIfPresent(dto, "project_hash", state.project_hash);
  assignIfPresent(dto, "host_id", state.host_id);

  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  if (state.range === "1d") {
    dto.since = formatLocalYmd(addLocalDays(today, -1));
    dto.until = formatLocalYmd(today);
  } else if (state.range === "7d") {
    dto.since = formatLocalYmd(addLocalDays(today, -6));
    dto.until = formatLocalYmd(today);
  } else if (state.range === "30d") {
    dto.since = formatLocalYmd(addLocalDays(today, -29));
    dto.until = formatLocalYmd(today);
  } else if (state.range === "custom") {
    assignIfPresent(dto, "since", state.since);
    assignIfPresent(dto, "until", state.until);
  }

  return dto;
}

export function windowFromState(state: FilterState): string {
  if (state.range === "custom") {
    return state.window || "day";
  }
  return RANGE_TO_TREND_WINDOW[state.range];
}

export function syncOptionsFromState(state: FilterState): SyncStartDto {
  const dto: SyncStartDto = {};
  if (state.source) {
    dto.source = state.source;
  }
  if (state.range === "1d") {
    dto.recent_days = 1;
  } else if (state.range === "7d") {
    dto.recent_days = 7;
  } else if (state.range === "30d") {
    dto.recent_days = 30;
  }
  return dto;
}

export function applyRange(state: FilterState, range: RangePreset): FilterState {
  if (range === "custom") {
    return { ...state, range, window: state.window || "day" };
  }
  return {
    ...state,
    range,
    window: RANGE_TO_TREND_WINDOW[range],
    since: undefined,
    until: undefined,
  };
}
