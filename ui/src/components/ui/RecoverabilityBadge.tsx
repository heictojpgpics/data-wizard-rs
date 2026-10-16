/**
 * RecoverabilityBadge — the per-file verdict pill (Excellent / Good /
 * Poor / Overwritten). Dot + label; tone drives the CSS var.
 */

import { RECOVERABILITY_META, recoverabilityOf } from "../../lib/format";
import type { RecoveryStatusDto } from "../../bindings";

export function RecoverabilityBadge({
  status,
  confidence,
  short,
}: {
  status: RecoveryStatusDto;
  confidence: number;
  short?: boolean;
}) {
  const rec = recoverabilityOf(status, confidence);
  const meta = RECOVERABILITY_META[rec];
  return (
    <span
      className={`dw-rec-badge dw-rec-${rec}`}
      data-tip={meta.hint}
      style={{ "--rec": `var(${meta.varName})`, "--rec-soft": `var(${meta.varName}-soft)` } as React.CSSProperties}
    >
      <span className="dw-rec-dot" aria-hidden />
      {short ? meta.label[0] : meta.label}
    </span>
  );
}
