import React from "react";
import { useTranslation } from "react-i18next";
import { LifeBuoy } from "lucide-react";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { SettingContainer } from "../../ui/SettingContainer";
import { Dropdown } from "../../ui/Dropdown";
import { useSettings } from "../../../hooks/useSettings";
import { useModelStore } from "../../../stores/modelStore";
import { getTranslatedModelName } from "../../../lib/utils/modelTranslation";

const NONE = "";

/** Models → If the dictation model fails: the second and third model, tried
 *  in order when a take cannot be transcribed (the server is down, the
 *  network dropped). Any downloaded or online model. */
export const FallbackSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const models = useModelStore((s) => s.models);
  const currentModel = useModelStore((s) => s.currentModel);
  const saved = (
    (getSetting("transcription_fallbacks") as string[]) ?? []
  ).filter((id) => id !== currentModel);

  const candidates = models.filter(
    (m) => m.is_downloaded && m.id !== currentModel,
  );
  const options = (exclude: string | undefined) => [
    { value: NONE, label: t("settings.models.fallback.none") },
    ...candidates
      .filter((m) => m.id !== exclude)
      .map((m) => ({ value: m.id, label: getTranslatedModelName(m, t) })),
  ];

  const set = (slot: 0 | 1, value: string) => {
    const next = [saved[0] ?? NONE, saved[1] ?? NONE];
    next[slot] = value;
    if (slot === 0 && value === NONE) next[1] = NONE;
    updateSetting(
      "transcription_fallbacks",
      next.filter((id) => id !== NONE),
    );
  };

  const busy = isUpdating("transcription_fallbacks");
  return (
    <SettingsGroup
      icon={LifeBuoy}
      title={t("settings.models.fallback.title")}
      description={t("settings.models.fallback.description")}
    >
      <SettingContainer
        title={t("settings.models.fallback.second.title")}
        description={t("settings.models.fallback.second.description")}
        descriptionMode="tooltip"
        grouped={true}
      >
        <Dropdown
          options={options(saved[1])}
          selectedValue={saved[0] ?? NONE}
          onSelect={(v) => set(0, v)}
          disabled={busy}
          className="w-72 [&>button]:w-full"
        />
      </SettingContainer>
      {saved[0] && (
        <SettingContainer
          title={t("settings.models.fallback.third.title")}
          description={t("settings.models.fallback.third.description")}
          descriptionMode="tooltip"
          grouped={true}
        >
          <Dropdown
            options={options(saved[0])}
            selectedValue={saved[1] ?? NONE}
            onSelect={(v) => set(1, v)}
            disabled={busy}
            className="w-72 [&>button]:w-full"
          />
        </SettingContainer>
      )}
    </SettingsGroup>
  );
};
