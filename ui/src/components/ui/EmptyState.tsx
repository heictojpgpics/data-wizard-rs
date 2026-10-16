/**
 * EmptyState — the teach-the-next-action pattern. Every empty view
 * (no results for filter, empty folder, search miss, no devices) shows
 * an illustration glyph, a headline, and concrete next steps.
 */

import type { ReactNode } from "react";

export function EmptyState({
  icon,
  title,
  body,
  actions,
  compact,
}: {
  icon: ReactNode;
  title: string;
  body?: ReactNode;
  actions?: ReactNode;
  compact?: boolean;
}) {
  return (
    <div className={`dw-empty ${compact ? "dw-compact" : ""}`} role="status">
      <div className="dw-empty-glyph" aria-hidden>
        {icon}
      </div>
      <h3>{title}</h3>
      {body ? <div className="dw-empty-body">{body}</div> : null}
      {actions ? <div className="dw-empty-actions">{actions}</div> : null}
    </div>
  );
}
