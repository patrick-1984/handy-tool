import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { ShortcutInput } from "../ShortcutInput";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { SettingContainer } from "../../ui/SettingContainer";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { Button } from "../../ui/Button";
import { useSettings } from "../../../hooks/useSettings";
import { useOsType } from "../../../hooks/useOsType";
import { commands, type AnchorStatus } from "@/bindings";

/**
 * The Jumper (Windows-only): five jump slots for desktop text fields. Slot 0
 * is the HOT slot — transcription flows can set/clear/jump/deliver into it and
 * it can auto-track where text last landed. Slots 1–4 are static bookmarks.
 * Slots live in memory only (window handles die with their windows).
 */
export const JumperSettings: React.FC = () => {
  const { t } = useTranslation();
  const osType = useOsType();
  const { getSetting, updateSetting, isUpdating } = useSettings();
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

  const keep = getSetting("anchor_keep") ?? false;
  const returnFocus = getSetting("anchor_return_focus") ?? true;

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
        <span className="text-sm">
          {t("settings.general.anchor.status.active", {
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
        <ToggleSwitch
          checked={keep}
          onChange={(enabled) => updateSetting("anchor_keep", enabled)}
          isUpdating={isUpdating("anchor_keep")}
          label={t("settings.general.anchor.keep.label")}
          description={t("settings.general.anchor.keep.description")}
          descriptionMode="tooltip"
          grouped={true}
        />
        <ToggleSwitch
          checked={returnFocus}
          onChange={(enabled) => updateSetting("anchor_return_focus", enabled)}
          isUpdating={isUpdating("anchor_return_focus")}
          label={t("settings.general.anchor.returnFocus.label")}
          description={t("settings.general.anchor.returnFocus.description")}
          descriptionMode="tooltip"
          grouped={true}
        />
      </SettingsGroup>
      {[1, 2, 3, 4].map((i) => (
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
        </SettingsGroup>
      ))}
    </div>
  );
};
