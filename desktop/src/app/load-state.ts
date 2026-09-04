import { allocateRequestId, invokeCommand } from "../runtime/invoke";
import { rangeToFilterDto, windowFromState } from "./filters";
import type { FilterState, InteractiveSnapshot } from "./types";

export const CORE_SLOW_MS = 2000;
export const CORE_FAIL_MS = 6000;

export type LoadControllerDeps = {
  invokeCommand?: typeof invokeCommand;
  allocateRequestId?: typeof allocateRequestId;
  now?: () => Date;
  slowMs?: number;
  failMs?: number;
  onSlow?: (generation: number) => void;
  onFail?: (generation: number, error: unknown) => void;
  onSnapshot?: (generation: number, snapshot: InteractiveSnapshot) => void;
};

export type LoadController = {
  generation: number;
  inflightIds: number[];
  loadDashboardProgressive: (state: FilterState) => Promise<InteractiveSnapshot | null>;
};

export function createLoadController(deps: LoadControllerDeps = {}): LoadController {
  const invoke = deps.invokeCommand ?? invokeCommand;
  const nextId = deps.allocateRequestId ?? allocateRequestId;
  const slowMs = deps.slowMs ?? CORE_SLOW_MS;
  const failMs = deps.failMs ?? CORE_FAIL_MS;

  const controller: LoadController = {
    generation: 0,
    inflightIds: [],
    async loadDashboardProgressive(state: FilterState) {
      controller.generation += 1;
      const generation = controller.generation;
      const previousIds = [...controller.inflightIds];
      const requestId = nextId();
      controller.inflightIds = [requestId];

      const filter = rangeToFilterDto(state, deps.now?.() ?? new Date());
      const window = windowFromState(state);

      let slowTimer: ReturnType<typeof setTimeout> | undefined;
      let failTimer: ReturnType<typeof setTimeout> | undefined;
      let timedOut = false;

      const clearTimers = () => {
        if (slowTimer !== undefined) clearTimeout(slowTimer);
        if (failTimer !== undefined) clearTimeout(failTimer);
      };

      const dropIfStale = () => {
        if (generation !== controller.generation) {
          controller.inflightIds = controller.inflightIds.filter((id) => id !== requestId);
        }
      };

      try {
        if (previousIds.length > 0) {
          await invoke("cancel_queries", { request: { request_ids: previousIds } });
        }
        if (generation !== controller.generation) {
          dropIfStale();
          return null;
        }

        slowTimer = setTimeout(() => {
          if (generation === controller.generation) {
            deps.onSlow?.(generation);
          }
        }, slowMs);

        const snapshot = await Promise.race([
          invoke<InteractiveSnapshot>("dashboard_interactive", {
            request: { request_id: requestId, filter, window },
          }),
          new Promise<never>((_, reject) => {
            failTimer = setTimeout(() => {
              timedOut = true;
              void invoke("cancel_queries", { request: { request_ids: [requestId] } });
              reject(Object.assign(new Error("dashboard query exceeded timeout"), { code: "timeout" }));
            }, failMs);
          }),
        ]);

        if (generation !== controller.generation || timedOut) {
          dropIfStale();
          return null;
        }
        clearTimers();
        deps.onSnapshot?.(generation, snapshot);
        return snapshot;
      } catch (error) {
        if (generation === controller.generation) {
          deps.onFail?.(generation, error);
        }
        return null;
      } finally {
        clearTimers();
        dropIfStale();
      }
    },
  };

  return controller;
}
