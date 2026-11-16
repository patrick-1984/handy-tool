import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ArrowDown, ArrowUp, PanelLeft, Pin, X } from "lucide-react";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { Button } from "../../ui/Button";
import { Dropdown } from "../../ui/Dropdown";
import { AppearanceSetting } from "../AppearanceSetting";
import { useSettings } from "../../../hooks/useSettings";
import {
  MORE_GROUPS,
  SECTIONS_CONFIG,
  type SidebarSection,
} from "../../Sidebar";
import {
  isValidShortcut,
  moveBy,
  orderedEntries,
} from "../../../lib/sidebarLayout";
import { PIN_ICONS, pinIcon } from "../../sidebar/pinIcons";
import { updateLayout, usePinLabel } from "../../sidebar/useSidebarLayout";
import type { SidebarShortcut } from "@/bindings";

const ICON =
  "inline-flex items-center justify-center h-7 w-7 rounded-md text-text-secondary hover:bg-hover hover:text-text cursor-pointer disabled:opacity-40 disabled:cursor-default disabled:hover:bg-transparent";

/** The 20 icons in a small grid under the row's icon button. */
const IconPicker: React.FC<{
  value: string;
  onPick: (key: string) => void;
}> = ({ value, onPick }) => {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [open]);
  const Current = pinIcon(value);
  return (
    <div ref={box} className="relative">
      <button
        type="button"
        className={`${ICON} border border-border`}
        onClick={() => setOpen((o) => !o)}
        title={t("settings.appearance.pins.icon")}
        aria-label={t("settings.appearance.pins.icon")}
        aria-expanded={open}
      >
        <Current width={16} height={16} />
      </button>
      {open && (
        <div
          role="listbox"
          className="absolute z-20 top-8 start-0 grid grid-cols-5 gap-1 p-2 rounded-lg bg-surface border border-border shadow-float"
        >
          {Object.entries(PIN_ICONS).map(([key, Icon]) => (
            <button
              key={key}
              type="button"
              role="option"
              aria-selected={key === value}
              aria-label={key}
              onClick={() => {
                onPick(key);
                setOpen(false);
              }}
              className={`inline-flex items-center justify-center h-8 w-8 rounded-md cursor-pointer ${
                key === value
                  ? "bg-accent-soft text-accent-text"
                  : "hover:bg-hover"
              }`}
            >
              <Icon width={16} height={16} />
            </button>
          ))}
        </div>
      )}
    </div>
  );
};

/** A pinned page's name: typed freely, saved when the field is left. */
const NameField: React.FC<{
  pin: SidebarShortcut;
  placeholder: string;
}> = ({ pin, placeholder }) => {
  const { t } = useTranslation();
  const [value, setValue] = useState(pin.name ?? "");
  useEffect(() => setValue(pin.name ?? ""), [pin.name]);
  const save = () => {
    const name = value.trim();
    if (name === (pin.name ?? "")) return;
    void updateLayout((layout) => ({
      ...layout,
      shortcuts: (layout.shortcuts ?? []).map((s) =>
        s.id === pin.id ? { ...s, name: name || null } : s,
      ),
    }));
  };
  return (
    <input
      type="text"
      value={value}
      maxLength={60}
      placeholder={placeholder}
      aria-label={t("settings.appearance.pins.name")}
      onChange={(e) => setValue(e.target.value)}
      onBlur={save}
      onKeyDown={(e) => {
        if (e.key === "Enter") (e.target as HTMLInputElement).blur();
      }}
      className="flex-1 min-w-0 h-8 px-2 rounded-md bg-surface2 border border-border text-sm"
    />
  );
};

/**
 * Advanced › Appearance (2.7.0): the theme, and the sidebar - which entries
 * show and which sit under "Show more", their order, and the pinned pages.
 */
export const AppearancePage: React.FC = () => {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const pinLabel = usePinLabel();
  const layout = settings?.sidebar_layout ?? {};

  const defaults = Object.entries(SECTIONS_CONFIG)
    .filter(([_, config]) => config.placement === "sidebar")
    .map(([id]) => id);
  defaults.splice(defaults.indexOf("jumper") + 1, 0, "more-tools");
  defaults.push("more-settings");
  const entries = orderedEntries(defaults, layout.order);
  const inMore = new Set(layout.in_more ?? []);
  const pins = layout.shortcuts ?? [];

  const entryLabel = (id: string) =>
    id in MORE_GROUPS
      ? t(MORE_GROUPS[id as keyof typeof MORE_GROUPS].labelKey)
      : t(SECTIONS_CONFIG[id as SidebarSection].labelKey);
  const entryIcon = (id: string) =>
    id in MORE_GROUPS
      ? MORE_GROUPS[id as keyof typeof MORE_GROUPS].icon
      : SECTIONS_CONFIG[id as SidebarSection].icon;
  const available = (pin: SidebarShortcut) => {
    const config = SECTIONS_CONFIG[pin.section as SidebarSection];
    return (
      isValidShortcut(pin, (id) => id in SECTIONS_CONFIG) &&
      !!config &&
      config.enabled(settings)
    );
  };

  const moveEntry = (index: number, by: -1 | 1) =>
    void updateLayout((l) => ({
      ...l,
      order: moveBy(orderedEntries(defaults, l.order), index, by),
    }));
  const setEntryInMore = (id: string, more: boolean) =>
    void updateLayout((l) => {
      const set = new Set(l.in_more ?? []);
      if (more) set.add(id);
      else set.delete(id);
      return { ...l, in_more: [...set] };
    });
  const changePin = (id: string, change: Partial<SidebarShortcut>) =>
    void updateLayout((l) => ({
      ...l,
      shortcuts: (l.shortcuts ?? []).map((s) =>
        s.id === id ? { ...s, ...change } : s,
      ),
    }));
  const movePin = (index: number, by: -1 | 1) =>
    void updateLayout((l) => ({
      ...l,
      shortcuts: moveBy(l.shortcuts ?? [], index, by),
    }));
  const removePin = (id: string) =>
    void updateLayout((l) => ({
      ...l,
      shortcuts: (l.shortcuts ?? []).filter((s) => s.id !== id),
    }));

  const placeOptions = [
    { value: "top", label: t("settings.appearance.place.top") },
    { value: "more", label: t("settings.appearance.place.more") },
  ];

  return (
    <div className="w-full flex flex-col gap-6">
      <AppearanceSetting descriptionMode="tooltip" grouped={false} />

      <SettingsGroup icon={Pin} title={t("settings.appearance.pins.title")}>
        <div className="px-4 py-3 flex flex-col gap-2">
          <p className="text-xs text-text-secondary">
            {t("settings.appearance.pins.description")}
          </p>
          {pins.length === 0 && (
            <p className="text-sm text-text-secondary">
              {t("settings.appearance.pins.empty")}
            </p>
          )}
          {pins.map((pin, index) => {
            const ok = available(pin);
            return (
              <div
                key={pin.id}
                className={`flex items-center gap-2 ${ok ? "" : "opacity-60"}`}
              >
                <IconPicker
                  value={pin.icon ?? "pin"}
                  onPick={(icon) => changePin(pin.id, { icon })}
                />
                <NameField
                  pin={pin}
                  placeholder={pinLabel({ ...pin, name: null })}
                />
                {!ok && (
                  <span className="text-xs text-text-secondary whitespace-nowrap">
                    {t("settings.appearance.pins.unavailable")}
                  </span>
                )}
                <Dropdown
                  options={placeOptions}
                  selectedValue={pin.in_more ? "more" : "top"}
                  onSelect={(value) =>
                    changePin(pin.id, { in_more: value === "more" })
                  }
                />
                <button
                  type="button"
                  className={ICON}
                  onClick={() => movePin(index, -1)}
                  disabled={index === 0}
                  title={t("settings.appearance.moveUp")}
                  aria-label={t("settings.appearance.moveUp")}
                >
                  <ArrowUp className="w-4 h-4" />
                </button>
                <button
                  type="button"
                  className={ICON}
                  onClick={() => movePin(index, 1)}
                  disabled={index === pins.length - 1}
                  title={t("settings.appearance.moveDown")}
                  aria-label={t("settings.appearance.moveDown")}
                >
                  <ArrowDown className="w-4 h-4" />
                </button>
                <button
                  type="button"
                  className={ICON}
                  onClick={() => removePin(pin.id)}
                  title={t("sidebar.pinned.unpin")}
                  aria-label={t("sidebar.pinned.unpin")}
                >
                  <X className="w-4 h-4" />
                </button>
              </div>
            );
          })}
        </div>
      </SettingsGroup>

      <SettingsGroup
        icon={PanelLeft}
        title={t("settings.appearance.sidebar.title")}
      >
        <div className="px-4 py-3 flex flex-col gap-1.5">
          <p className="text-xs text-text-secondary">
            {t("settings.appearance.sidebar.description")}
          </p>
          {entries.map((id, index) => {
            const Icon = entryIcon(id);
            return (
              <div key={id} className="flex items-center gap-2 h-9">
                <Icon width={16} height={16} className="shrink-0" />
                <span className="flex-1 min-w-0 text-sm truncate">
                  {entryLabel(id)}
                </span>
                <Dropdown
                  options={placeOptions}
                  selectedValue={inMore.has(id) ? "more" : "top"}
                  onSelect={(value) => setEntryInMore(id, value === "more")}
                />
                <button
                  type="button"
                  className={ICON}
                  onClick={() => moveEntry(index, -1)}
                  disabled={index === 0}
                  title={t("settings.appearance.moveUp")}
                  aria-label={t("settings.appearance.moveUp")}
                >
                  <ArrowUp className="w-4 h-4" />
                </button>
                <button
                  type="button"
                  className={ICON}
                  onClick={() => moveEntry(index, 1)}
                  disabled={index === entries.length - 1}
                  title={t("settings.appearance.moveDown")}
                  aria-label={t("settings.appearance.moveDown")}
                >
                  <ArrowDown className="w-4 h-4" />
                </button>
              </div>
            );
          })}
          <div className="pt-2">
            <Button
              variant="secondary"
              size="sm"
              onClick={() =>
                void updateLayout((l) => ({ ...l, order: [], in_more: [] }))
              }
              disabled={(layout.order ?? []).length === 0 && inMore.size === 0}
            >
              {t("settings.appearance.sidebar.reset")}
            </Button>
          </div>
        </div>
      </SettingsGroup>
    </div>
  );
};
