import React, { useEffect } from "react";
import { Info, Pause, TextQuote } from "lucide-react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { SubSettings } from "../ui/SettingsGroup";
import { ShortcutInput } from "./ShortcutInput";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";
import { takeShortcutsAvailable } from "../../lib/utils/keyboard";
import { useModelStore } from "../../stores/modelStore";
import { toast } from "sonner";
import type { ModelInfo } from "@/bindings";

/** The live text box's widths on offer; wider means fewer lines. */
const LIVE_TEXT_WIDTHS = [
  { px: 360, key: "narrow" },
  { px: 460, key: "medium" },
  { px: 640, key: "wide" },
  { px: 860, key: "extraWide" },
];

/** The live text box's text sizes on offer, in px. */
const LIVE_TEXT_SIZES = [
  { px: 13, key: "small" },
  { px: 15, key: "normal" },
  { px: 18, key: "large" },
  { px: 22, key: "extraLarge" },
];

/** "Whole text": the box heights on offer, in lines. */
const LIVE_TEXT_HEIGHTS = [
  { lines: 3, key: "small" },
  { lines: 6, key: "medium" },
  { lines: 10, key: "large" },
  { lines: 16, key: "extraLarge" },
];

/** Remote engines get the audio only after stop, so their takes are never live. */
export const isRemote = (model?: ModelInfo) =>
  model?.engine_type === "ApiWhisper" ||
  model?.engine_type === "OpenRouterWhisper";

interface Props {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/** Pause/resume button on the recording overlay; its shortcut shows once it is on. */
export const PauseButtonSetting: React.FC<Props> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const osType = useOsType();
  const enabled = getSetting("pause_button_enabled") ?? false;
  return (
    <>
      <ToggleSwitch
        lead
        checked={enabled}
        onChange={(value) => updateSetting("pause_button_enabled", value)}
        icon={Pause}
        isUpdating={isUpdating("pause_button_enabled")}
        label={t("settings.general.pauseButton.label")}
        description={t("settings.general.pauseButton.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
      {/* The pill's button works everywhere; its shortcut not on Linux. */}
      {enabled && takeShortcutsAvailable(osType) && (
        <SubSettings>
          <ShortcutInput shortcutId="pause" grouped={grouped} />
        </SubSettings>
      )}
    </>
  );
};

/**
 * What live transcription costs, in plain view while it is on (the live text
 * box, or Transcription Mode = Live): most people never open an (i) tooltip.
 */
export const LiveTradeoffsNotice: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const osType = useOsType();
  const live =
    (getSetting("live_text_box_enabled") ?? false) ||
    getSetting("transcription_mode") === "live";
  if (!live) return null;
  return (
    // Sits between cards: the card above closes and the next one starts below.
    <div className="card-break mt-4">
      <div className="flex gap-3 rounded-lg border border-info-border bg-info-bg px-4 py-3">
        <Info
          className="w-4 h-4 mt-0.5 shrink-0 text-accent-text"
          aria-hidden
        />
        <div className="space-y-1 text-[13px] leading-[18px]">
          <p className="font-semibold">
            {t("settings.general.liveTradeoffs.title")}
          </p>
          <ul className="list-disc ps-4 space-y-0.5">
            <li>{t("settings.general.liveTradeoffs.accuracy")}</li>
            <li>{t("settings.general.liveTradeoffs.cpu")}</li>
            {takeShortcutsAvailable(osType) && (
              <li>{t("settings.general.liveTradeoffs.undo")}</li>
            )}
          </ul>
        </div>
      </div>
    </div>
  );
};

/** Undo-last-word shortcut for live takes; its shortcut shows once it is on. */
export const UndoWordSetting: React.FC<Props> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const osType = useOsType();
  const enabled = getSetting("undo_word_enabled") ?? false;
  // Undo works only through its shortcut, which Linux never registers.
  if (!takeShortcutsAvailable(osType)) return null;
  return (
    <>
      <ToggleSwitch
        lead
        checked={enabled}
        onChange={(value) => updateSetting("undo_word_enabled", value)}
        isUpdating={isUpdating("undo_word_enabled")}
        label={t("settings.general.undoWord.label")}
        description={t("settings.general.undoWord.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
      {enabled && (
        <SubSettings>
          <ShortcutInput shortcutId="undo_word" grouped={grouped} />
        </SubSettings>
      )}
    </>
  );
};

/**
 * Live text box next to the recording pill, what it shows, and whether it fades
 * when you stop talking. The overlay's T button flips the same setting, so
 * follow its change event.
 */
export const LiveTextBoxSetting: React.FC<Props> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating, refreshSettings } =
    useSettings();
  const enabled = getSetting("live_text_box_enabled") ?? false;
  const mode = getSetting("live_text_mode") ?? "last_words";
  const fade = getSetting("live_text_fade") ?? false;
  const models = useModelStore((state) => state.models);
  const current = models.find((m) => m.id === getSetting("selected_model"));
  const remote = isRemote(current);

  const toggle = (value: boolean) => {
    if (value && remote) {
      // It cannot work with this model: say so, and which models it works with.
      const local = models
        .filter((m) => m.is_downloaded && !isRemote(m))
        .map((m) => m.name);
      toast.warning(
        t("settings.general.liveTextBox.remoteModel.title", {
          model: current?.name,
        }),
        {
          description: local.length
            ? t("settings.general.liveTextBox.remoteModel.worksWith", {
                models: local.join(", "),
              })
            : t("settings.general.liveTextBox.remoteModel.noLocal"),
          duration: 10000,
        },
      );
      return;
    }
    void updateSetting("live_text_box_enabled", value);
  };

  useEffect(() => {
    const unlisten = listen("live-text-box-changed", () => {
      void refreshSettings();
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [refreshSettings]);

  return (
    <>
      <ToggleSwitch
        lead
        // An online model cannot use it: switching it on shows nothing more.
        noRevealHint={remote}
        checked={enabled}
        onChange={toggle}
        icon={TextQuote}
        isUpdating={isUpdating("live_text_box_enabled")}
        label={t("settings.general.liveTextBox.label")}
        description={t("settings.general.liveTextBox.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
      {enabled && (
        <SubSettings>
          {remote && (
            <p className="px-4 py-2 text-xs text-warn-text">
              {t("settings.general.liveTextBox.remoteModel.title", {
                model: current?.name,
              })}
            </p>
          )}
          <SettingContainer
            title={t("settings.general.liveTextMode.title")}
            description={t("settings.general.liveTextMode.description")}
            descriptionMode={descriptionMode}
            grouped={grouped}
          >
            <Dropdown
              options={[
                {
                  value: "last_words",
                  label: t("settings.general.liveTextMode.lastWords"),
                },
                {
                  value: "full_text",
                  label: t("settings.general.liveTextMode.fullText"),
                },
              ]}
              selectedValue={mode}
              onSelect={(value) =>
                updateSetting(
                  "live_text_mode",
                  value as "last_words" | "full_text",
                )
              }
              disabled={isUpdating("live_text_mode")}
            />
          </SettingContainer>
          <SettingContainer
            title={t("settings.general.liveTextWidth.title")}
            description={t("settings.general.liveTextWidth.description")}
            descriptionMode={descriptionMode}
            grouped={grouped}
          >
            <Dropdown
              options={LIVE_TEXT_WIDTHS.map(({ px, key }) => ({
                value: String(px),
                label: t(`settings.general.liveTextWidth.${key}`),
              }))}
              selectedValue={String(getSetting("live_text_width") ?? 460)}
              onSelect={(value) =>
                updateSetting("live_text_width", Number(value))
              }
              disabled={isUpdating("live_text_width")}
            />
          </SettingContainer>
          {mode === "full_text" && (
            <SettingContainer
              title={t("settings.general.liveTextHeight.title")}
              description={t("settings.general.liveTextHeight.description")}
              descriptionMode={descriptionMode}
              grouped={grouped}
            >
              <Dropdown
                options={LIVE_TEXT_HEIGHTS.map(({ lines, key }) => ({
                  value: String(lines),
                  label: t(`settings.general.liveTextHeight.${key}`),
                }))}
                selectedValue={String(getSetting("live_text_lines") ?? 6)}
                onSelect={(value) =>
                  updateSetting("live_text_lines", Number(value))
                }
                disabled={isUpdating("live_text_lines")}
              />
            </SettingContainer>
          )}
          <SettingContainer
            title={t("settings.general.liveTextSize.title")}
            description={t("settings.general.liveTextSize.description")}
            descriptionMode={descriptionMode}
            grouped={grouped}
          >
            <Dropdown
              options={LIVE_TEXT_SIZES.map(({ px, key }) => ({
                value: String(px),
                label: t(`settings.general.liveTextSize.${key}`),
              }))}
              selectedValue={String(getSetting("live_text_font_size") ?? 15)}
              onSelect={(value) =>
                updateSetting("live_text_font_size", Number(value))
              }
              disabled={isUpdating("live_text_font_size")}
            />
          </SettingContainer>
          <ShortcutInput shortcutId="toggle_live_text_box" grouped={grouped} />
          <ToggleSwitch
            checked={fade}
            onChange={(value) => updateSetting("live_text_fade", value)}
            isUpdating={isUpdating("live_text_fade")}
            label={t("settings.general.liveTextFade.label")}
            description={t("settings.general.liveTextFade.description")}
            descriptionMode={descriptionMode}
            grouped={grouped}
          />
        </SubSettings>
      )}
    </>
  );
};
