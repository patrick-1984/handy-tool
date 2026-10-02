import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";
import {
  BookOpen,
  BrainCircuit,
  FileUp,
  FolderUp,
  MessageSquareText,
  Trash2,
} from "lucide-react";
import { toast } from "sonner";
import { commands, type LlmProvider, type NoteSkill } from "@/bindings";
import { LANGUAGE_METADATA } from "@/i18n/languages";
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
import { Textarea } from "../../ui/Textarea";
import { NoteModelSelect } from "./NoteModelSelect";
import { translateNoteError } from "./noteErrors";
import { SpeakerDetectionSettings } from "./SpeakerDetectionSettings";

const SKILL_FILE_EXTENSIONS = ["md", "markdown", "txt", "zip", "skill"];

/** Turn one skill on or off; the active list keeps the skills list's order. */
export const toggleSkillIds = (
  skills: NoteSkill[],
  activeIds: string[],
  id: string,
  on: boolean,
): string[] => {
  const next = new Set(activeIds.filter((a) => a !== id));
  if (on) next.add(id);
  return skills.map((skill) => skill.id).filter((sid) => next.has(sid));
};

/** "Your instructions" and the note language. */
const InstructionsGroup: React.FC = () => {
  const { t } = useTranslation();
  const { settings, updateSetting } = useSettings();
  const saved = settings?.note_custom_instructions ?? "";
  const [text, setText] = useState(saved);
  useEffect(() => setText(saved), [saved]);

  const languageOptions: DropdownOption[] = [
    { value: "", label: t("settings.notes.language.sameAsTranscript") },
    ...Object.entries(LANGUAGE_METADATA)
      .sort(
        ([, a], [, b]) => (a.priority ?? Infinity) - (b.priority ?? Infinity),
      )
      .map(([code, meta]) => ({ value: code, label: meta.nativeName })),
  ];

  return (
    <SettingsGroup
      icon={MessageSquareText}
      title={t("settings.notes.instructions.title")}
      description={t("settings.notes.instructions.description")}
    >
      <SettingContainer
        title={t("settings.notes.instructions.custom.title")}
        description={t("settings.notes.instructions.custom.description")}
        descriptionMode="tooltip"
        layout="stacked"
        grouped={true}
      >
        <Textarea
          value={text}
          onChange={(e) => setText(e.target.value)}
          onBlur={() => {
            if (text !== saved)
              void updateSetting("note_custom_instructions", text);
          }}
          placeholder={t("settings.notes.instructions.custom.placeholder")}
          className="w-full min-h-[110px] select-text"
        />
      </SettingContainer>
      <SettingContainer
        title={t("settings.notes.language.title")}
        description={t("settings.notes.language.description")}
        descriptionMode="tooltip"
        grouped={true}
      >
        <Dropdown
          className="min-w-[240px]"
          options={languageOptions}
          selectedValue={settings?.note_language ?? ""}
          onSelect={(value) => void updateSetting("note_language", value)}
        />
      </SettingContainer>
    </SettingsGroup>
  );
};

const SkillsGroup: React.FC = () => {
  const { t } = useTranslation();
  const { settings, updateSetting, refreshSettings } = useSettings();
  const skills = useNotesStore((state) => state.skills);
  const loadSkills = useNotesStore((state) => state.loadSkills);
  const [importing, setImporting] = useState(false);

  useEffect(() => {
    void loadSkills();
  }, [loadSkills]);

  const activeIds = settings?.note_skill_ids ?? [];

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

  const removeSkill = async (skill: NoteSkill) => {
    try {
      const result = await commands.deleteNoteSkill(skill.id);
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
        layout="stacked"
        grouped={true}
      >
        {skills.length === 0 ? (
          <p className="text-[13px] text-text-secondary">
            {t("settings.notes.skills.none")}
          </p>
        ) : (
          <ul className="flex flex-col gap-1">
            {skills.map((skill) => {
              const on = activeIds.includes(skill.id);
              return (
                <li
                  key={skill.id}
                  className="flex items-start gap-2.5 rounded-md px-1 py-1 hover:bg-hover"
                >
                  <input
                    id={`note-skill-${skill.id}`}
                    type="checkbox"
                    checked={on}
                    disabled={importing}
                    onChange={(e) =>
                      void updateSetting(
                        "note_skill_ids",
                        toggleSkillIds(
                          skills,
                          activeIds,
                          skill.id,
                          e.target.checked,
                        ),
                      )
                    }
                    className="mt-0.5 w-4 h-4 shrink-0 accent-accent cursor-pointer"
                  />
                  <label
                    htmlFor={`note-skill-${skill.id}`}
                    className="min-w-0 flex-1 cursor-pointer"
                  >
                    <span className="block text-sm break-words">
                      {skill.name}
                    </span>
                    <span className="block text-xs text-text-secondary break-words">
                      {skill.description ??
                        t("settings.notes.skills.fileCount", {
                          count: skill.file_count,
                        })}
                    </span>
                    {skill.truncated && (
                      <span className="block text-xs text-warn-text">
                        {t("settings.notes.skills.truncated")}
                      </span>
                    )}
                  </label>
                  <button
                    type="button"
                    onClick={() => void removeSkill(skill)}
                    className={`${ICON_BUTTON} hover:!bg-err-bg hover:!text-err-text`}
                    title={t("settings.notes.skills.remove")}
                  >
                    <Trash2 width={16} height={16} />
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </SettingContainer>
      <div className="px-4 py-3 flex flex-col gap-2">
        <p className="text-[13px] text-text-secondary break-words">
          {activeIds.length > 0
            ? t("settings.notes.skills.activeSummary", {
                count: activeIds.length,
              })
            : (settings?.note_custom_instructions ?? "").trim() !== ""
              ? t("settings.notes.skills.onlyOwnInstructions")
              : t("settings.notes.skills.defaultDescription")}
        </p>
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
  const { settings, updateSetting, refreshSettings } = useSettings();
  const navigateTo = useNavStore((state) => state.navigateTo);

  // Switching to a provider of another kind also resets the note model
  // (backend), so reload the settings to show it.
  const selectProvider = async (id: string) => {
    await updateSetting("note_provider_ref", id);
    await refreshSettings();
  };

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
          onSelect={(value) => void selectProvider(value ?? "")}
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
        <NoteModelSelect
          value={settings?.note_model ?? ""}
          provider={provider}
          onCommit={(value) => updateSetting("note_model", value.trim())}
          placeholder={
            provider?.model || t("settings.notes.provider.model.placeholder")
          }
          className="w-[320px]"
          showSelectedPrice
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

/**
 * Notes › Settings: your instructions and the note language, the skills,
 * the provider and model, then speaker detection for "Make note with
 * speakers".
 */
export const NoteSettingsTab: React.FC = () => (
  <div className="space-y-6">
    <InstructionsGroup />
    <SkillsGroup />
    <ProviderGroup />
    <SpeakerDetectionSettings />
  </div>
);
