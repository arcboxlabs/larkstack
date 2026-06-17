import { Select as BaseSelect } from "@base-ui/react/select";
import type { CSSProperties, ReactNode } from "react";

export interface SelectOption {
  value: string;
  label: ReactNode;
}

/// A thin wrapper over Base UI `Select` for single-value string selects: pass
/// `options` and a controlled `value`/`onValueChange`. `className`/`style`
/// target the trigger — default is the pill `.select-trigger` (Events filters);
/// form fields pass `"field-input field-select"` to match the other inputs.
export function Select({
  value,
  onValueChange,
  options,
  id,
  name,
  disabled,
  className = "select-trigger",
  style,
}: {
  value: string;
  onValueChange: (value: string) => void;
  options: readonly SelectOption[];
  id?: string;
  name?: string;
  disabled?: boolean;
  className?: string;
  style?: CSSProperties;
}) {
  const labelOf = (v: string): ReactNode =>
    options.find((o) => o.value === v)?.label ?? v;
  return (
    <BaseSelect.Root
      modal={false}
      value={value}
      disabled={disabled}
      name={name}
      onValueChange={(v) => onValueChange((v as string | null) ?? "")}
    >
      <BaseSelect.Trigger id={id} className={className} style={style}>
        <BaseSelect.Value>{(v) => labelOf(v as string)}</BaseSelect.Value>
        <BaseSelect.Icon className="select-icon">▾</BaseSelect.Icon>
      </BaseSelect.Trigger>
      <BaseSelect.Portal>
        <BaseSelect.Positioner sideOffset={4} align="start">
          <BaseSelect.Popup className="select-popup">
            {options.map((o) => (
              <BaseSelect.Item
                key={o.value}
                value={o.value}
                className="select-item"
              >
                <BaseSelect.ItemIndicator className="select-item-indicator">
                  ✓
                </BaseSelect.ItemIndicator>
                <BaseSelect.ItemText>{o.label}</BaseSelect.ItemText>
              </BaseSelect.Item>
            ))}
          </BaseSelect.Popup>
        </BaseSelect.Positioner>
      </BaseSelect.Portal>
    </BaseSelect.Root>
  );
}
