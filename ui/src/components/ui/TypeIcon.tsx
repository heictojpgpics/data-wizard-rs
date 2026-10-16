/**
 * TypeIcon — the per-file glyph: category-colored square badge with the
 * category line-icon, plus the extension caption where requested. The
 * EaseUS file rows carry a 24px per-extension icon; we render the same
 * role with category coloring (extensions inside a category share the
 * glyph — 40 SVGs in theirs collapse visually anyway).
 */

import { CategoryGlyph } from "../../lib/Icon";
import { categoryOf } from "../../lib/format";

export function TypeIcon({
  name,
  extension,
  size = 24,
  badge = true,
}: {
  /** File name (drives the "no-extension" fallback). */
  name: string;
  extension: string;
  size?: number;
  /** Solid tinted badge vs. bare glyph. */
  badge?: boolean;
}) {
  const ext = (extension || (name.includes(".") ? name.split(".").pop()! : "")).toLowerCase();
  const cat = categoryOf(ext);
  if (!badge) {
    return <CategoryGlyph cat={cat} size={size} />;
  }
  return (
    <span
      className="dw-type-icon"
      data-cat={cat}
      style={{ width: size, height: size }}
      aria-hidden
    >
      <CategoryGlyph cat={cat} size={Math.round(size * 0.62)} />
    </span>
  );
}
