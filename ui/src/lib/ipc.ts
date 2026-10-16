/**
 * Unified invoke + event bus.
 *
 * Product mode: Tauri IPC. Browser-dev/test mode (no `__TAURI__`, DEV
 * build only): the mock backend from `src/mock` — a synthetic recovery
 * scenario that implements the exact command surface, so the entire
 * UI is exercisable in a plain browser (Playwright/agent-browser
 * screenshot passes) with zero product-code mock data (the mock is
 * never bundled into a Tauri production build: dynamic import behind
 * isTauri() + DEV gates in main.tsx).
 */
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen, type UnlistenFn } from "@tauri-apps/api/event";

export type { UnlistenFn };

export function isTauri(): boolean {
  return typeof window !== "undefined" && ("__TAURI_INTERNALS__" in window || "__TAURI__" in window);
}

type MockCommand = (cmd: string, args: Record<string, unknown> | undefined) => Promise<unknown>;

let mockCommand: MockCommand | null = null;
const mockListeners = new Map<string, Set<(payload: unknown) => void>>();

export function setMockBackend(handler: MockCommand | null): void {
  mockCommand = handler;
}

export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (mockCommand) {
    return (await mockCommand(cmd, args)) as T;
  }
  return await tauriInvoke<T>(cmd, args);
}

/** Event subscription — routes to the local mock bus when installed. */
export async function listen<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  if (mockCommand) {
    let set = mockListeners.get(event);
    if (!set) {
      set = new Set<(payload: unknown) => void>();
      mockListeners.set(event, set);
    }
    const fn = handler as (payload: unknown) => void;
    set.add(fn);
    return () => {
      set?.delete(fn);
    };
  }
  return tauriListen<T>(event, (e) => handler(e.payload));
}

/** Emit an event on the mock bus (mock-internal use only). */
export function emitMockEvent(event: string, payload: unknown): void {
  const set = mockListeners.get(event);
  if (set) {
    for (const fn of set) fn(payload);
  }
}
