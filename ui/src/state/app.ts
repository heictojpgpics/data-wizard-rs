/**
 * App store — the wizard stage machine + the current scan's identity
 * (source, deep flag, timing anchor). Deliberately THIN: browsing state
 * lives in state/results.ts, the scan lifecycle in state/scan.ts,
 * devices in state/devices.ts, recovery in state/recovery.ts. This is
 * the join point the shell reads.
 */

import { create } from "zustand";

export type Stage = "source" | "scanning" | "results";

/** A source chosen for scanning (disk, partition, or image file). */
export interface ChosenSource {
  /** The URI passed to start_scan (disk id / \\.\ path / file path). */
  uri: string;
  /** Display name for headers. */
  label: string;
  /** Sub-label (bus / fs kind / size). */
  detail: string;
  /** Optional partition index within the source. */
  partition: number | null;
  /** Optional path-prefix filter applied post-scan (folder quick target). */
  pathPrefix?: string;
}

interface AppStore {
  stage: Stage;
  setStage: (s: Stage) => void;

  scanId: string | null;
  scanDeep: boolean;
  source: ChosenSource | null;
  /** Wall-clock ms when the current scan began (elapsed displays). */
  scanStartedAt: number;
  beginScan: (scanId: string, deep: boolean, source: ChosenSource) => void;
  clearScan: () => void;
}

export const useAppStore = create<AppStore>((set) => ({
  stage: "source",
  setStage: (s) => set({ stage: s }),

  scanId: null,
  scanDeep: false,
  source: null,
  scanStartedAt: 0,

  beginScan: (scanId, deep, source) =>
    set({
      scanId,
      scanDeep: deep,
      source,
      scanStartedAt: Date.now(),
      stage: "scanning",
    }),

  clearScan: () =>
    set({
      scanId: null,
      source: null,
      stage: "source",
      scanStartedAt: 0,
    }),
}));
