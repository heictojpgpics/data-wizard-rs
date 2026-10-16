/**
 * UI store — toasts (multi, stacked, auto-dismiss), the confirm-dialog
 * slot (one at a time; confirms are modal), and misc transient chrome
 * state. Views never own dialog state locally: they route through here
 * so the layering stays testable.
 */

import { create } from "zustand";
import type { ConfirmSpec } from "../components/ui/ConfirmDialog";

export interface Toast {
  id: number;
  kind: "ok" | "err" | "info" | "warn";
  title: string;
  body?: string;
  /** ms until auto-dismiss; 0 = sticky. */
  ttl: number;
  closing?: boolean;
}

interface UiStore {
  toasts: Toast[];
  pushToast: (t: Omit<Toast, "id" | "ttl" | "closing"> & { ttl?: number }) => void;
  beginCloseToast: (id: number) => void;
  dropToast: (id: number) => void;

  confirm: ConfirmSpec | null;
  openConfirm: (spec: ConfirmSpec) => void;
  closeConfirm: () => void;
}

let nextToastId = 1;

export const useUiStore = create<UiStore>((set, get) => ({
  toasts: [],

  pushToast: (t) => {
    const id = nextToastId++;
    const ttl = t.ttl ?? (t.kind === "err" ? 8000 : 4500);
    set((st) => ({ toasts: [...st.toasts, { ...t, id, ttl }] }));
    if (ttl > 0) {
      window.setTimeout(() => get().beginCloseToast(id), ttl);
    }
  },

  beginCloseToast: (id) => {
    set((st) => ({
      toasts: st.toasts.map((t) => (t.id === id && !t.closing ? { ...t, closing: true } : t)),
    }));
    window.setTimeout(() => get().dropToast(id), 260);
  },

  dropToast: (id) => set((st) => ({ toasts: st.toasts.filter((t) => t.id !== id) })),

  confirm: null,
  openConfirm: (spec) => set({ confirm: spec }),
  closeConfirm: () => set({ confirm: null }),
}));
