/**
 * CapacityBar — the storage meter used on drive cards and destination
 * rows. Two-segment (used/free) with optional capacity annotation.
 * The bar is a plain div stack (no SVG) so theme transitions animate.
 */

import { formatBytes } from "../../lib/format";

export function CapacityBar({
  total,
  free,
  label,
  height = 8,
  showLabels = false,
}: {
  total: number;
  free: number | null;
  label?: string;
  height?: number;
  showLabels?: boolean;
}) {
  const used = free == null ? null : Math.max(0, total - free);
  const pct = used == null || total <= 0 ? null : Math.min(100, (used / total) * 100);
  return (
    <div className="dw-cap" aria-label={label ?? "Capacity"}>
      <div className="dw-cap-track" style={{ height }}>
        {pct != null ? (
          <div
            className="dw-cap-used"
            style={{
              width: `${pct.toFixed(2)}%`,
              /* Squeeze the amber/red warning zones at 85%+ used. */
              ...(pct > 95 ? { background: "var(--used)" } : {}),
            }}
          />
        ) : (
          <div className="dw-cap-unknown" style={{ width: "100%" }} />
        )}
      </div>
      {showLabels ? (
        <div className="dw-cap-labels tnum">
          <span>
            {free == null ? "Capacity " : `${formatBytes(free)} free of `}
            {formatBytes(total)}
          </span>
        </div>
      ) : null}
    </div>
  );
}
