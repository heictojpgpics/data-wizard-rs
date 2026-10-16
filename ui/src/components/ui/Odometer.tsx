/**
 * Odometer — the animated number renderer (the EaseUS "Files Found"
 * counter feel). Digits ride vertical rails; when the value changes the
 * rails glide to the new digits with the overshoot ease.
 *
 * Reduced motion is handled globally (the 0.01ms clamp collapses the
 * rail transition to an instant swap).
 */

import { memo, useMemo } from "react";

export interface OdometerProps {
  value: number;
  /** Format applied to the FULL value (thousands separators etc.). */
  format?: (n: number) => string;
  className?: string;
}

/** memo: parent re-renders on every progress tick; digit decomposition
 * only changes when the formatted string actually changes. */
export const Odometer = memo(function Odometer({
  value,
  format,
  className,
}: OdometerProps) {
  const text = useMemo(
    () => (format ? format(value) : value.toLocaleString(undefined)),
    [value, format],
  );

  return (
    <span className={`dw-odo tnum ${className ?? ""}`} aria-label={text}>
      {text.split("").map((ch, i) =>
        /\d/.test(ch) ? (
          <span className="dw-odo-digit" key={`${i}-${ch}`} aria-hidden>
            <span className="dw-odo-rail" style={{ transform: `translateY(-${Number(ch)}em)` }}>
              {[0, 1, 2, 3, 4, 5, 6, 7, 8, 9].map((d) => (
                <span key={d}>{d}</span>
              ))}
            </span>
          </span>
        ) : (
          <span key={`${i}-${ch}`} aria-hidden>
            {ch}
          </span>
        ),
      )}
    </span>
  );
});
