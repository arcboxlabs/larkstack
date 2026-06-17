import { Checkbox as BaseCheckbox } from "@base-ui/react/checkbox";
import type { Ref } from "react";

/// A controlled Base UI checkbox styled to match the dashboard. Use
/// `checked`/`onCheckedChange`; pass `inputRef`/`name` to bind it to
/// react-hook-form via `<Controller>`.
export function Checkbox({
  checked,
  onCheckedChange,
  disabled,
  id,
  name,
  inputRef,
}: {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  disabled?: boolean;
  id?: string;
  name?: string;
  inputRef?: Ref<HTMLInputElement>;
}) {
  return (
    <BaseCheckbox.Root
      className="checkbox"
      checked={checked}
      onCheckedChange={onCheckedChange}
      disabled={disabled}
      id={id}
      name={name}
      inputRef={inputRef}
    >
      <BaseCheckbox.Indicator className="checkbox-indicator">
        ✓
      </BaseCheckbox.Indicator>
    </BaseCheckbox.Root>
  );
}
