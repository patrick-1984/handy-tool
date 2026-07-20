import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { ShortcutInput } from "../ShortcutInput";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { SettingContainer } from "../../ui/SettingContainer";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { Dropdown } from "../../ui/Dropdown";
import { Button } from "../../ui/Button";
import { useSettings } from "../../../hooks/useSettings";
import { useOsType } from "../../../hooks/useOsType";
import { commands, type AnchorStatus, type CursorMode } from "@/bindings";

/**
 * The Jumper (Windows-only): eleven jump slots for desktop text fields. Slots 0
 * (Hot 1) and 10 (Hot 2) are the HOT slots — transcription flows can
 * set/clear/jump/deliver into either; any slot can auto-track where text last
 * landed. Slots 1–9 are static bookmarks. Live slots are in-memory; the opt-in
 * persistence setting below saves slot identities and re-resolves them across
 * restarts.
 *
 * Cursor save/restore is PER-SLOT (T-302): each slot (hot + 1–9) has its own
 * "save mouse position" switch. The cursor position mode is now ALSO per-slot
 * (T-304): each slot has its own mode dropdown, disabled when that slot's save
 * toggle is off. When a slot has save enabled, delivering into (or tracking
 * onto) it captures the pointer so a jump restores it.
 */
export const JumperSettings: React.FC = () => {
  const { t } = useTranslation();
  const osType = useOsType();
  const { getSetting, updateSetting, isUpdating, refreshSettings } =
    useSettings();
  const [slots, setSlots] = useState<(AnchorStatus | null)[]>([]);

  // Delivery-failure toasts are handled globally in App.tsx — this page only
  // tracks slot occupancy.
  useEffect(() => {
    if (osType !== "windows") return;
    let disposed = false;
    commands.getJumpSlots().then((s) => {
      if (!disposed) setSlots(s);
    });
    const unlistenChanged = listen<(AnchorStatus | null)[]>(
      "anchor-changed",
      (e) => setSlots(e.payload),
    );
    return () => {
      disposed = true;
      unlistenChanged.then((f) => f());
    };
  }, [osType]);

  if (osType !== "windows") {
    return (
      <div className="w-full px-4 py-6 text-sm text-mid-gray">
        {t("settings.jumper.windowsOnly")}
      </div>
    );
  }

  const testSlot = async (index: number) => {
    const result = await commands.jumpToSlot(index);
    if (result.status === "error") {
      toast.error(
        t("settings.jumper.slot.testFailed", { reason: result.error }),
      );
    }
  };

  const slotStatus = (index: number) => {
    const s = slots[index];
    return s ? (
      <div className="flex items-center gap-3">
        <span className={`text-sm ${s.stale ? "text-red-400" : ""}`}>
          {s.stale
            ? t("settings.jumper.slot.stale", {
                app: s.app,
                control: s.control_class,
              })
            : t("settings.general.anchor.status.active", {
                app: s.app,
                control: s.control_class,
              })}
        </span>
        <Button size="sm" variant="secondary" onClick={() => testSlot(index)}>
          {t("settings.jumper.slot.test")}
        </Button>
        <Button
          size="sm"
          variant="secondary"
          onClick={() => commands.clearJumpSlot(index)}
        >
          {t("settings.general.anchor.status.clear")}
        </Button>
      </div>
    ) : (
      <span className="text-sm text-mid-gray">
        {t("settings.general.anchor.status.none")}
      </span>
    );
  };

  // Per-slot save-cursor state. The setting is a bool[] of length SLOT_COUNT=11
  // (index = slot; 0 = Hot 1, 1–9 = static, 10 = Hot 2); missing/short reads false.
  const saveCursorSlots = getSetting("jumper_save_cursor_slots") as
    | boolean[]
    | undefined;
  const isSaveCursorOn = (index: number) => saveCursorSlots?.[index] ?? false;

  const setSaveCursor = async (index: number, value: boolean) => {
    try {
      const result = await commands.changeJumperSaveCursorSlot(index, value);
      if (result.status === "error") {
        toast.error(String(result.error));
        return;
      }
      await refreshSettings();
    } catch (error) {
      toast.error(String(error));
    }
  };

  const saveCursorToggle = (index: number) => (
    <ToggleSwitch
      checked={isSaveCursorOn(index)}
      onChange={(value) => void setSaveCursor(index, value)}
      label={t("settings.jumper.saveCursor.perSlot")}
      description={t("settings.jumper.saveCursor.perSlotDescription")}
      descriptionMode="tooltip"
      grouped={true}
    />
  );

  // Per-slot cursor-position mode (T-304). Vector of CursorMode, index = slot;
  // a missing/short entry reads "AppRelative".
  const cursorModeSlots = getSetting("jumper_cursor_mode_slots") as
    | CursorMode[]
    | undefined;
  const cursorModeFor = (index: number): string =>
    String(cursorModeSlots?.[index] ?? "AppRelative");
  const cursorModeOptions = [
    {
      value: "AppRelative",
      label: t("settings.jumper.saveCursor.mode.appRelative"),
    },
    {
      value: "ScreenAbsolute",
      label: t("settings.jumper.saveCursor.mode.screenAbsolute"),
    },
  ];
  const setCursorModeSlot = async (index: number, value: string) => {
    try {
      const result = await commands.changeJumperCursorModeSlot(index, value);
      if (result.status === "error") {
        toast.error(String(result.error));
        return;
      }
      await refreshSettings();
    } catch (error) {
      toast.error(String(error));
    }
  };

  // Rendered under each slot's save toggle; disabled (grayed) when that slot's
  // save-cursor switch is off, so the mode is always visible but inert until
  // the slot actually saves a cursor (T-304).
  const cursorModeDropdown = (index: number) => (
    <SettingContainer
      title={t("settings.jumper.saveCursor.mode.title")}
      description={t("settings.jumper.saveCursor.mode.description")}
      descriptionMode="tooltip"
      grouped={true}
    >
      <Dropdown
        options={cursorModeOptions}
        selectedValue={cursorModeFor(index)}
        onSelect={(value) => void setCursorModeSlot(index, value)}
        disabled={!isSaveCursorOn(index)}
      />
    </SettingContainer>
  );

  // On-finish flow-match gate (T-302 #3).
  const requireSameFlow =
    (getSetting("anchor_on_finish_require_same_flow") as boolean | undefined) ??
    false;
  const setRequireSameFlow = async (value: boolean) => {
    try {
      const result = await commands.changeAnchorRequireSameFlow(value);
      if (result.status === "error") {
        toast.error(String(result.error));
        return;
      }
      await refreshSettings();
    } catch (error) {
      toast.error(String(error));
    }
  };

  return (
    <div className="w-full space-y-6">
      <SettingsGroup title={t("settings.jumper.hot.title")}>
        <ShortcutInput shortcutId="anchor_set" grouped={true} />
        <ShortcutInput shortcutId="anchor_jump" grouped={true} />
        <SettingContainer
          title={t("settings.general.anchor.status.title")}
          description={t("settings.general.anchor.status.description")}
          descriptionMode="tooltip"
          grouped={true}
        >
          {slotStatus(0)}
        </SettingContainer>
        {saveCursorToggle(0)}
        {cursorModeDropdown(0)}
        <SettingContainer
          title={t("settings.jumper.hot.optionsMoved.title")}
          description={t("settings.jumper.hot.optionsMoved.description")}
          descriptionMode="tooltip"
          grouped={true}
        >
          <span className="text-sm text-mid-gray">
            {t("settings.jumper.hot.optionsMoved.hint")}
          </span>
        </SettingContainer>
      </SettingsGroup>
      <SettingsGroup title={t("settings.jumper.hot2.title")}>
        <ShortcutInput shortcutId="anchor_set_2" grouped={true} />
        <ShortcutInput shortcutId="anchor_jump_2" grouped={true} />
        <SettingContainer
          title={t("settings.general.anchor.status.title")}
          description={t("settings.general.anchor.status.description")}
          descriptionMode="tooltip"
          grouped={true}
        >
          {slotStatus(10)}
        </SettingContainer>
        {saveCursorToggle(10)}
        {cursorModeDropdown(10)}
      </SettingsGroup>
      <SettingsGroup title={t("settings.jumper.persist.groupTitle")}>
        <ToggleSwitch
          checked={getSetting("jumper_persist") ?? false}
          onChange={(enabled) => updateSetting("jumper_persist", enabled)}
          isUpdating={isUpdating("jumper_persist")}
          label={t("settings.jumper.persist.label")}
          description={t("settings.jumper.persist.description")}
          descriptionMode="tooltip"
          grouped={true}
        />
      </SettingsGroup>
      <SettingsGroup title={t("settings.jumper.requireSameFlow.groupTitle")}>
        <ToggleSwitch
          checked={requireSameFlow}
          onChange={(value) => void setRequireSameFlow(value)}
          label={t("settings.jumper.requireSameFlow.label")}
          description={t("settings.jumper.requireSameFlow.description")}
          descriptionMode="tooltip"
          grouped={true}
        />
      </SettingsGroup>
      {[1, 2, 3, 4, 5, 6, 7, 8, 9].map((i) => (
        <SettingsGroup
          key={i}
          title={t("settings.jumper.slot.title", { index: i })}
        >
          <ShortcutInput shortcutId={`jump_set_slot_${i}`} grouped={true} />
          <ShortcutInput shortcutId={`jump_slot_${i}`} grouped={true} />
          <SettingContainer
            title={t("settings.jumper.slot.status", { index: i })}
            description={t("settings.jumper.slot.description")}
            descriptionMode="tooltip"
            grouped={true}
          >
            {slotStatus(i)}
          </SettingContainer>
          {saveCursorToggle(i)}
          {cursorModeDropdown(i)}
        </SettingsGroup>
      ))}
    </div>
  );
};
