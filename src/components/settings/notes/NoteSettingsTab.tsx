import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";
import { BookOpen, BrainCircuit, FileUp, FolderUp, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { commands, type LlmProvider } from "@/bindings";
import { useSettings } from "@/hooks/useSettings";
import { useNavStore } from "@/stores/navStore";
import {
  NOTE_PROVIDER_KINDS,
  noteProvider,
  useNotesStore,
} from "@/stores/notesStore";
import { Button } from "../../ui/Button";
import { Dropdown, type DropdownOption } from "../../ui/Dropdown";
import { SettingContainer } from "../../ui/SettingContainer";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { ICON_BUTTON, TEXT_FIELD } from "../../ui/controlClasses";
import { SearchableModelSelect } from "../SearchableModelSelect";
import { translateNoteError } from "./noteErrors";

const SKILL_FILE_EXTENSIONS = ["md", "markdown", "txt", "zip", "skill"];

/** The default instructions, then the imported skills. */
export const useNoteSkillOptions = (): DropdownOption[] => {
  const { t } = useTranslation();
  const skills = useNotesStore((state) => state.skills);
  return [
    { value: "", label: t("settings.notes.defaultSkill") },
    ...skills.map((skill) => ({ value: skill.id, label: skill.name })),
  ];
};

const SkillsGroup: React.FC = () => {
  const { t } = useTranslation();
  const { settings, updateSetting, refreshSettings } = useSettings();
  const skills = useNotesStore((state) => state.skills);
  const loadSkills = useNotesStore((state) => state.loadSkills);
  const options = useNoteSkillOptions();
  const [importing, setImporting] = useState(false);

  useEffect(() => {
    void loadSkills();
  }, [loadSkills]);

  const selectedId = settings?.note_skill_id ?? "";
  const selected = skills.find((skill) => skill.id === selectedId);

  const importSkill = async (directory: boolean) => {
    const picked = await open(
      directory
        ? { directory: true, multiple: false }
        : {
            directory: false,
            multiple: false,
            filters: [
              {
                name: t("settings.notes.skills.fileFilter"),
                extensions: SKILL_FILE_EXTENSIONS,
              },
            ],
          },
    );
    if (typeof picked !== "string") return;

    setImporting(true);
    try {
      const result = await commands.importNoteSkill(picked);
      if (result.status === "ok") {
        toast.success(
          t("settings.notes.skills.imported", { name: result.data.name }),
        );
        await Promise.all([loadSkills(), refreshSettings()]);
      } else {
        toast.error(
          t("settings.notes.skills.importError", {
            error: translateNoteError(result.error, t),
          }),
        );
      }
    } catch (error) {
      toast.error(
        t("settings.notes.skills.importError", { error: String(error) }),
      );
    } finally {
      setImporting(false);
    }
  };

  const removeSkill = async () => {
    if (!selected) return;
    try {
      const result = await commands.deleteNoteSkill(selected.id);
      if (result.status !== "ok") throw new Error(result.error);
      await Promise.all([loadSkills(), refreshSettings()]);
    } catch (error) {
      console.error("Failed to remove skill:", error);
      toast.error(t("settings.notes.skills.removeError"));
    }
  };

  return (
    <SettingsGroup
      icon={BookOpen}
      title={t("settings.notes.skills.title")}
      description={t("settings.notes.skills.description")}
    >
      <SettingContainer
        title={t("settings.notes.skills.active.title")}
        description={t("settings.notes.skills.active.description")}
        descriptionMode="tooltip"
        grouped={true}
      >
        <div className="flex items-center gap-1">
          <Dropdown
            className="min-w-[240px]"
            options={options}
            selectedValue={selected ? selected.id : ""}
            onSelect={(value) => updateSetting("note_skill_id", value || null)}
            disabled={importing}
          />
          {selected && (
            <button
              type="button"
              onClick={removeSkill}
              className={`${ICON_BUTTON} hover:!bg-err-bg hover:!text-err-text`}
              title={t("settings.notes.skills.remove")}
            >
              <Trash2 width={16} height={16} />
            </button>
          )}
        </div>
      </SettingContainer>
      <div className="px-4 py-3 flex flex-col gap-2">
        <p className="text-[13px] text-text-secondary break-words">
          {selected
            ? (selected.description ??
              t("settings.notes.skills.fileCount", {
                count: selected.file_count,
              }))
            : t("settings.notes.skills.defaultDescription")}
        </p>
        {selected?.truncated && (
          <p className="text-[13px] text-warn-text">
            {t("settings.notes.skills.truncated")}
          </p>
        )}
        <div className="flex flex-wrap gap-2">
          <Button
            variant="secondary"
            size="sm"
            onClick={() => void importSkill(false)}
            disabled={importing}
          >
            <FileUp className="w-3.5 h-3.5" />
            {t("settings.notes.skills.importFile")}
          </Button>
          <Button
            variant="secondary"
            size="sm"
            onClick={() => void importSkill(true)}
            disabled={importing}
          >
            <FolderUp className="w-3.5 h-3.5" />
            {t("settings.notes.skills.importFolder")}
          </Button>
        </div>
        <p className="text-xs text-text-secondary">
          {t("settings.notes.skills.importHint")}
        </p>
      </div>
    </SettingsGroup>
  );
};

/** The selected provider's key, edited in the provider registry itself. */
const ApiKeyField: React.FC<{ provider: LlmProvider }> = ({ provider }) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();
  const [value, setValue] = useState(provider.api_key ?? "");
  useEffect(() => setValue(provider.api_key ?? ""), [provider.api_key]);

  const commit = () => {
    const trimmed = value.trim();
    if (trimmed === (provider.api_key ?? "")) return;
    const providers =
      (getSetting("llm_providers") as LlmProvider[] | undefined) ?? [];
    updateSetting(
      "llm_providers",
      providers.map((p) =>
        p.id === provider.id ? { ...p, api_key: trimmed } : p,
      ),
    );
  };

  return (
    <input
      type="password"
      value={value}
      onChange={(e) => setValue(e.target.value)}
      onBlur={commit}
      placeholder={t("settings.notes.provider.apiKey.placeholder")}
      className={`${TEXT_FIELD} w-[320px]`}
    />
  );
};

const ProviderGroup: React.FC = () => {
  const { t } = useTranslation();
  const { settings, updateSetting } = useSettings();
  const navigateTo = useNavStore((state) => state.navigateTo);

  const providers = settings?.llm_providers ?? [];
  const provider = noteProvider(settings);
  // The registry index "#N" matches Advanced settings › LLM providers.
  const options = providers
    .map((p, idx) => ({ provider: p, idx }))
    .filter(({ provider: p }) => NOTE_PROVIDER_KINDS.includes(p.kind))
    .map(({ provider: p, idx }) => ({
      value: p.id,
      label: `#${idx + 1} · ${p.name}${p.model ? ` · ${p.model}` : ""}`,
    }));

  return (
    <SettingsGroup
      icon={BrainCircuit}
      title={t("settings.notes.provider.title")}
      description={t("settings.notes.provider.description")}
    >
      <SettingContainer
        title={t("settings.notes.provider.provider.title")}
        description={t("settings.notes.provider.provider.description")}
        descriptionMode="tooltip"
        grouped={true}
      >
        <Dropdown
          selectedValue={provider?.id ?? null}
          options={options}
          onSelect={(value) => updateSetting("note_provider_ref", value ?? "")}
          placeholder={t("settings.notes.provider.provider.placeholder")}
          className="min-w-[320px]"
        />
      </SettingContainer>
      {provider && (
        <SettingContainer
          title={t("settings.notes.provider.apiKey.title")}
          description={t("settings.notes.provider.apiKey.description")}
          descriptionMode="tooltip"
          grouped={true}
        >
          <ApiKeyField provider={provider} />
        </SettingContainer>
      )}
      <SettingContainer
        title={t("settings.notes.provider.model.title")}
        description={t("settings.notes.provider.model.description")}
        descriptionMode="tooltip"
        grouped={true}
      >
        <SearchableModelSelect
          value={settings?.note_model ?? ""}
          providerId={provider?.id ?? null}
          onCommit={(value) => updateSetting("note_model", value.trim())}
          placeholder={
            provider?.model || t("settings.notes.provider.model.placeholder")
          }
          className="w-[320px]"
        />
      </SettingContainer>
      <div className="px-4 py-2.5">
        <button
          type="button"
          onClick={() => navigateTo("llmProviders")}
          className="text-xs text-accent-text hover:underline cursor-pointer"
        >
          {t("settings.notes.provider.manage")}
        </button>
      </div>
    </SettingsGroup>
  );
};

/** Notes › Settings: the skill, then the provider and model. */
export const NoteSettingsTab: React.FC = () => (
  <div className="space-y-6">
    <SkillsGroup />
    <ProviderGroup />
  </div>
);
