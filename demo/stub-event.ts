const listeners = new Map<string, Set<(event: { payload: unknown }) => void>>();

export async function listen(
  event: string,
  handler: (event: { payload: unknown }) => void,
): Promise<() => void> {
  const set = listeners.get(event) ?? new Set();
  set.add(handler);
  listeners.set(event, set);
  return () => set.delete(handler);
}

export function emit(event: string, payload: unknown): void {
  for (const handler of listeners.get(event) ?? []) handler({ payload });
}
