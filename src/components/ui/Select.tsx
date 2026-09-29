import React from "react";
import SelectComponent from "react-select";
import CreatableSelect from "react-select/creatable";
import type {
  ActionMeta,
  Props as ReactSelectProps,
  SingleValue,
  StylesConfig,
} from "react-select";

export type SelectOption = {
  value: string;
  label: string;
  isDisabled?: boolean;
};

type BaseProps = {
  value: string | null;
  options: SelectOption[];
  placeholder?: string;
  disabled?: boolean;
  isLoading?: boolean;
  isClearable?: boolean;
  onChange: (value: string | null, action: ActionMeta<SelectOption>) => void;
  onBlur?: () => void;
  className?: string;
  formatCreateLabel?: (input: string) => string;
};

type CreatableProps = {
  isCreatable: true;
  onCreateOption: (value: string) => void;
};

type NonCreatableProps = {
  isCreatable?: false;
  onCreateOption?: never;
};

export type SelectProps = BaseProps & (CreatableProps | NonCreatableProps);

// The redesign's control: 32px, a darker bottom edge that turns into a 2px
// cyan line while open or focused; the menu floats on a surface card.
const selectStyles: StylesConfig<SelectOption, false> = {
  control: (base, state) => ({
    ...base,
    minHeight: 32,
    borderRadius: 6,
    borderColor: "var(--color-control-border)",
    borderBottomColor: "var(--color-control-bottom)",
    boxShadow: state.isFocused ? "inset 0 -2px 0 var(--color-accent)" : "none",
    backgroundColor: state.isDisabled
      ? "var(--color-dis-bg)"
      : "var(--color-control)",
    fontSize: "0.875rem",
    color: "var(--color-text)",
    transition: "background-color 150ms ease",
    ":hover": {
      borderColor: "var(--color-control-border)",
      borderBottomColor: "var(--color-control-bottom)",
      backgroundColor: "var(--color-control-hover)",
    },
  }),
  valueContainer: (base) => ({
    ...base,
    paddingInline: 10,
    paddingBlock: 0,
  }),
  input: (base) => ({
    ...base,
    color: "var(--color-text)",
  }),
  singleValue: (base) => ({
    ...base,
    color: "var(--color-text)",
  }),
  indicatorSeparator: () => ({ display: "none" }),
  dropdownIndicator: (base) => ({
    ...base,
    padding: 6,
    color: "var(--color-text-secondary)",
    ":hover": {
      color: "var(--color-text)",
    },
  }),
  clearIndicator: (base) => ({
    ...base,
    padding: 6,
    color: "var(--color-text-secondary)",
    ":hover": {
      color: "var(--color-text)",
    },
  }),
  menu: (provided) => ({
    ...provided,
    zIndex: 30,
    padding: 4,
    borderRadius: 8,
    backgroundColor: "var(--color-surface)",
    color: "var(--color-text)",
    border: "1px solid var(--color-border)",
    boxShadow: "var(--shadow-float)",
  }),
  option: (base, state) => ({
    ...base,
    borderRadius: 6,
    backgroundColor: state.isSelected
      ? "var(--color-active)"
      : state.isFocused
        ? "var(--color-hover)"
        : "transparent",
    fontWeight: state.isSelected ? 600 : 400,
    color: state.isDisabled ? "var(--color-dis-text)" : "var(--color-text)",
    cursor: state.isDisabled ? "not-allowed" : base.cursor,
  }),
  placeholder: (base) => ({
    ...base,
    color: "var(--color-text-secondary)",
  }),
};

export const Select: React.FC<SelectProps> = React.memo(
  ({
    value,
    options,
    placeholder,
    disabled,
    isLoading,
    isClearable = true,
    onChange,
    onBlur,
    className = "",
    isCreatable,
    formatCreateLabel,
    onCreateOption,
  }) => {
    const selectValue = React.useMemo(() => {
      if (!value) return null;
      const existing = options.find((option) => option.value === value);
      if (existing) return existing;
      return { value, label: value, isDisabled: false };
    }, [value, options]);

    const handleChange = (
      option: SingleValue<SelectOption>,
      action: ActionMeta<SelectOption>,
    ) => {
      onChange(option?.value ?? null, action);
    };

    const sharedProps: Partial<ReactSelectProps<SelectOption, false>> = {
      className,
      classNamePrefix: "app-select",
      value: selectValue,
      options,
      onChange: handleChange,
      placeholder,
      isDisabled: disabled,
      isLoading,
      onBlur,
      isClearable,
      styles: selectStyles,
    };

    if (isCreatable) {
      return (
        <CreatableSelect<SelectOption, false>
          {...sharedProps}
          onCreateOption={onCreateOption}
          formatCreateLabel={formatCreateLabel}
        />
      );
    }

    return <SelectComponent<SelectOption, false> {...sharedProps} />;
  },
);

Select.displayName = "Select";
