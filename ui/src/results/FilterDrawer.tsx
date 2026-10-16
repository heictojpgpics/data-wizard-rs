/**
 * FilterDrawer — the advanced filter panel (EaseUS
 * CAdvanceFileFilterMgr parity): type tree with counts + checkboxes,
 * size ranges, date ranges, custom extension query, and the
 * junk-visibility toggles. Slides over the file list; every control
 * writes straight into the results store's filter.
 */

import {
  CalendarIcon,
  FilterIcon,
  RotateCcwIcon,
  XIcon,
  CategoryGlyph,
} from "../lib/Icon";
import {
  CATEGORY_ORDER_SAFE,
  DATE_RANGES,
  SIZE_RANGES,
  type CategoryId,
} from "../lib/format";
import { useResultsStore } from "../state/results";

export function FilterDrawer({ open, onClose }: { open: boolean; onClose: () => void }) {
  const filter = useResultsStore((s) => s.filter);
  const setFilter = useResultsStore((s) => s.setFilter);
  const clearFilter = useResultsStore((s) => s.clearFilter);
  const catCount = useResultsStore((s) => s.catCount);

  function toggleCategory(c: CategoryId) {
    const next = new Set(filter.categories);
    if (next.has(c)) next.delete(c);
    else next.add(c);
    setFilter({ categories: next });
  }

  function toggleSize(id: string) {
    const next = new Set(filter.sizeRanges);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setFilter({ sizeRanges: next });
  }

  return (
    <aside className={`dw-filter-drawer${open ? " dw-open" : ""}`} aria-hidden={!open}>
      <div className="dw-fd-head">
        <FilterIcon size={15} />
        <span>Filter &amp; search</span>
        <button className="dw-icon-btn dw-sm" aria-label="Close filters" onClick={onClose}>
          <XIcon size={14} />
        </button>
      </div>

      <div className="dw-fd-scroll">
        {/* File type */}
        <section className="dw-fd-section">
          <h4>File type</h4>
          <div className="dw-fd-checks">
            {CATEGORY_ORDER_SAFE.map((c) => {
              const count = catCount.get(c)?.files ?? 0;
              const on = filter.categories.has(c);
              return (
                <label key={c} className={`dw-fd-check${count === 0 ? " dw-zero" : ""}`}>
                  <input
                    type="checkbox"
                    className="dw-check"
                    checked={on}
                    onChange={() => toggleCategory(c)}
                  />
                  <span className="dw-fd-check-icon">
                    <CategoryGlyph cat={c} size={14} />
                  </span>
                  <span className="dw-fd-check-label">
                    {c[0].toUpperCase() + c.slice(1)}
                  </span>
                  <span className="dw-fd-check-count tnum">{count.toLocaleString()}</span>
                </label>
              );
            })}
          </div>
        </section>

        {/* File size */}
        <section className="dw-fd-section">
          <h4>File size</h4>
          <div className="dw-fd-checks dw-fd-cols1">
            {SIZE_RANGES.map((r) => {
              const on = filter.sizeRanges.has(r.id);
              return (
                <label key={r.id} className="dw-fd-check">
                  <input
                    type="checkbox"
                    className="dw-check"
                    checked={on}
                    onChange={() => toggleSize(r.id)}
                  />
                  <span className="dw-fd-check-label">{r.label}</span>
                </label>
              );
            })}
          </div>
        </section>

        {/* Date modified */}
        <section className="dw-fd-section">
          <h4>
            <CalendarIcon size={13} /> Date modified
          </h4>
          <div className="dw-fd-checks dw-fd-cols1">
            {DATE_RANGES.map((r) => (
              <label key={r.id} className="dw-fd-check">
                <input
                  type="radio"
                  name="dw-fd-date"
                  className="dw-check"
                  checked={filter.dateRange === r.id}
                  onChange={() => setFilter({ dateRange: r.id })}
                />
                <span className="dw-fd-check-label">{r.label}</span>
              </label>
            ))}
            <label className="dw-fd-check">
              <input
                type="radio"
                name="dw-fd-date"
                className="dw-check"
                checked={filter.dateRange == null}
                onChange={() => setFilter({ dateRange: null })}
              />
              <span className="dw-fd-check-label">Any date</span>
            </label>
          </div>
        </section>

        {/* Extensions */}
        <section className="dw-fd-section">
          <h4>File extensions</h4>
          <input
            className="dw-input dw-fd-ext"
            placeholder="*.doc; *.xlsx; *.raw"
            value={filter.extensions}
            onChange={(e) => setFilter({ extensions: e.target.value })}
            spellCheck={false}
          />
          <p className="dw-fd-hint">Separate with semicolons. Leave empty for all types.</p>
        </section>

        {/* Status */}
        <section className="dw-fd-section">
          <h4>Recovery status</h4>
          <div className="dw-fd-checks dw-fd-cols1">
            <label className="dw-fd-check">
              <input
                type="checkbox"
                className="dw-check"
                checked={filter.deletedOnly}
                onChange={(e) => setFilter({ deletedOnly: e.target.checked })}
              />
              <span className="dw-fd-check-label">Deleted files only</span>
            </label>
            <label className="dw-fd-check">
              <input
                type="checkbox"
                className="dw-check"
                checked={filter.hideOverwritten}
                onChange={(e) => setFilter({ hideOverwritten: e.target.checked })}
              />
              <span className="dw-fd-check-label">Hide overwritten (unrecoverable)</span>
            </label>
          </div>
        </section>

        {/* Junk toggles */}
        <section className="dw-fd-section">
          <h4>Clutter</h4>
          <div className="dw-fd-checks dw-fd-cols1">
            <label className="dw-fd-check">
              <input
                type="checkbox"
                className="dw-check"
                checked={filter.hideJunk}
                onChange={(e) => setFilter({ hideJunk: e.target.checked })}
              />
              <span className="dw-fd-check-label">Hide junk &amp; 0-byte files</span>
            </label>
            <label className="dw-fd-check">
              <input
                type="checkbox"
                className="dw-check"
                checked={filter.hideSystem}
                onChange={(e) => setFilter({ hideSystem: e.target.checked })}
              />
              <span className="dw-fd-check-label">Hide system files</span>
            </label>
          </div>
        </section>
      </div>

      <div className="dw-fd-foot">
        <button className="dw-btn-ghost dw-sm" onClick={clearFilter}>
          <RotateCcwIcon size={13} />
          Reset filter
        </button>
        <button className="dw-btn-ink dw-sm" onClick={onClose}>
          Show results
        </button>
      </div>
    </aside>
  );
}
