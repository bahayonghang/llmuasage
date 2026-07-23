export const SECONDARY_SECTIONS = Object.freeze(['activity', 'tools', 'optimize', 'explorer', 'compare']);
export const CORE_SLOW_MS = 2000;
export const CORE_TIMEOUT_MS = 6000;

export function armDashboardCoreDeadline({ setTimer, clearTimer, onSlow, onTimeout }) {
  const slowTimer = setTimer(onSlow, CORE_SLOW_MS);
  const timeoutTimer = setTimer(onTimeout, CORE_TIMEOUT_MS);
  return () => {
    clearTimer(slowTimer);
    clearTimer(timeoutTimer);
  };
}

export async function runLoadersWithConcurrency(loaders, concurrency, onResult) {
  const entries = Object.entries(loaders);
  let nextIndex = 0;
  const worker = async () => {
    while (nextIndex < entries.length) {
      const [section, load] = entries[nextIndex++];
      let payload = null;
      let loadError = null;
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

export function createDashboardLoadState(generation = 0, startedAtMs = Date.now()) {
  return {
    phase: 'core_pending',
    startedAtMs,
    slow: false,
    generation,
    secondaryTotal: SECONDARY_SECTIONS.length,
    secondarySettled: 0,
    secondaryDegraded: 0,
    settledSections: [],
    degradedSections: [],
    errorKind: null,
    errorMessage: null,
  };
}

export function reduceDashboardLoadState(state, event) {
  if (!state || event?.type === 'retry') {
    return createDashboardLoadState(event?.generation ?? ((state?.generation || 0) + 1), event?.startedAtMs);
  }
  if (event?.generation !== undefined && event.generation !== state.generation) return state;

  switch (event?.type) {
    case 'slow':
      return state.phase === 'core_pending' ? { ...state, slow: true } : state;
    case 'core_succeeded':
      return {
        ...state,
        phase: 'secondary_loading',
        slow: false,
        errorKind: null,
        errorMessage: null,
      };
    case 'secondary_settled': {
      if (state.phase !== 'secondary_loading' || !SECONDARY_SECTIONS.includes(event.section)) return state;
      if (state.settledSections.includes(event.section)) return state;
      const settledSections = [...state.settledSections, event.section];
      const secondarySettled = settledSections.length;
      return {
        ...state,
        phase: secondarySettled === state.secondaryTotal ? 'complete' : 'secondary_loading',
        secondarySettled,
        secondaryDegraded: state.secondaryDegraded + (event.degraded ? 1 : 0),
        settledSections,
        degradedSections: event.degraded ? [...state.degradedSections, event.section] : state.degradedSections,
      };
    }
    case 'failed':
      return {
        ...state,
        phase: 'error',
        slow: false,
        errorKind: event.errorKind || 'network',
        errorMessage: event.message || null,
      };
    default:
      return state;
  }
}

export function classifyDashboardError(error, timedOut = false) {
  if (timedOut) return 'timeout';
  if (error?.name === 'AbortError') return 'cancelled';
  if (Number.isInteger(error?.status)) return 'http';
  if (error instanceof SyntaxError) return 'parse';
  return 'network';
}

export function isDegradedSectionPayload(payload) {
  return payload?.support?.level === 'degraded';
}

export function dashboardLocaleRenderMode(state) {
  if (state?.loadState?.phase === 'error') return 'error';
  if (state?.rawData) return 'data';
  return 'loading';
}
