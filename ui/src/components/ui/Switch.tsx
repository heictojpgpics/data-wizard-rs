/**
 * Switch — the settings-toggle control (deep-scan default, auto-recover,
 * junk-hide toggles, live-sync prefs). Label + description optional.
 */

export function Switch({
  checked,
  onChange,
  label,
  disabled,
}: {
  checked: boolean;
  onChange: (on: boolean) => void;
  label?: string;
  disabled?: boolean;
}) {
  return (
    <label className={`dw-switch ${disabled ? "dw-disabled" : ""}`}>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={label}
        disabled={disabled}
        className="dw-switch-track"
        onClick={() => onChange(!checked)}
      >
        <span className="dw-switch-thumb" />
      </button>
      {label ? <span className="dw-switch-label">{label}</span> : null}
    </label>
  );
}

/**
 * SegmentedControl — the compact view-mode picker (list/grid, Local tab
 * groups). Renders as a joined pill row; keyboard: arrows move when
 * focused (roving via buttons is enough at this size).
 */
export function SegmentedControl<T extends string>({
  value,
  options,
  onChange,
  ariaLabel,
}: {
  value: T;
  options: { value: T; label: string; icon?: React.ReactNode; tip?: string }[];
  onChange: (v: T) => void;
  ariaLabel: string;
}) {
  return (
    <div className="dw-seg" role="radiogroup" aria-label={ariaLabel}>
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={value === o.value}
          data-tip={o.tip}
          className={`dw-seg-item ${value === o.value ? "dw-active" : ""}`}
          onClick={() => onChange(o.value)}
        >
          {o.icon}
          {o.label ? <span>{o.label}</span> : null}
        </button>
      ))}
    </div>
  );
}
