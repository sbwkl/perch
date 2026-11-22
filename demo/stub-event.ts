type ScanStatusEvent = {
  source: string;
  trigger: "manual" | "live";
  phase: "start" | "end";
  duration_ms: number | null;
  live_paused: boolean;
};

const listeners = new Map<string, Set<(event: { payload: unknown }) => void>>();

export async function listen(
  event: string,
  handler: (event: { payload: unknown }) => void,
): Promise<() => void> {
  const set = listeners.get(event) ?? new Set();
  set.add(handler);
  listeners.set(event, set);
  if (event === "scan-status") {
    const status: ScanStatusEvent = {
      source: "all",
      trigger: "manual",
      phase: "end",
      duration_ms: 12,
      live_paused: false,
    };
    window.setTimeout(() => handler({ payload: status }), 120);
  }
  return () => set.delete(handler);
}

export function emit(event: string, payload: unknown): void {
  for (const handler of listeners.get(event) ?? []) handler({ payload });
}
