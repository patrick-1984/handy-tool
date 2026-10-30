import React from "react";
import { useTranslation } from "react-i18next";
import { Combine, TextQuote } from "lucide-react";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { SettingContainer } from "../../ui/SettingContainer";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { Dropdown } from "../../ui/Dropdown";
import { AffixSettings } from "../AffixSettings";
import { useSettings } from "../../../hooks/useSettings";
import type { CombineSeparator } from "@/bindings";

const SEPARATORS: CombineSeparator[] = ["blank_line", "new_line", "space"];

/** Advanced settings › Combine recordings: how History joins several
 *  recordings copied as one text. */
export const CombineSection: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const keep = (getSetting("combine_keep_take_affixes") as boolean) ?? false;
  const separator = ((getSetting("combine_separator") as CombineSeparator) ??
    "blank_line") as CombineSeparator;
  return (
    <div className="space-y-4">
      <SettingsGroup
        icon={Combine}
        title={t("settings.combine.title")}
        description={t("settings.combine.description")}
      >
        <SettingContainer
          title={t("settings.combine.separator.title")}
          description={t("settings.combine.separator.description")}
          descriptionMode="tooltip"
          grouped={true}
        >
          <Dropdown
            options={SEPARATORS.map((value) => ({
              value,
              label: t(`settings.combine.separator.options.${value}`),
            }))}
            selectedValue={separator}
            onSelect={(v) =>
              updateSetting("combine_separator", v as CombineSeparator)
            }
            disabled={isUpdating("combine_separator")}
          />
        </SettingContainer>
        <ToggleSwitch
          checked={keep}
          onChange={(v) => updateSetting("combine_keep_take_affixes", v)}
          isUpdating={isUpdating("combine_keep_take_affixes")}
          label={t("settings.combine.keepTakeAffixes.label")}
          description={t("settings.combine.keepTakeAffixes.description")}
          descriptionMode="tooltip"
          grouped={true}
        />
      </SettingsGroup>
      <SettingsGroup
        icon={TextQuote}
        title={t("settings.combine.affixes.title")}
        description={t("settings.combine.affixes.description")}
      >
        <AffixSettings flow="combine" grouped={true} />
      </SettingsGroup>
    </div>
  );
};
