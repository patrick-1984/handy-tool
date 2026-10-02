import React, { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "@/hooks/useSettings";
import { useNotesStore } from "@/stores/notesStore";
import { Dropdown } from "../../ui/Dropdown";
import { useNoteSkillOptions } from "./NoteSettingsTab";

/**
 * A compact switch for the skill notes are written with, for History's
 * toolbar. Hidden until a skill is imported.
 */
export const NoteSkillPicker: React.FC = () => {
  const { t } = useTranslation();
  const { settings, updateSetting } = useSettings();
  const skills = useNotesStore((state) => state.skills);
  const loadSkills = useNotesStore((state) => state.loadSkills);
  const options = useNoteSkillOptions();

  useEffect(() => {
    void loadSkills();
  }, [loadSkills]);

  if (skills.length === 0) return null;

  const selectedId = settings?.note_skill_id ?? "";
  const selected = skills.some((skill) => skill.id === selectedId)
    ? selectedId
    : "";

  return (
    <div className="flex items-center gap-2 min-w-0">
      <span className="text-xs text-text-secondary shrink-0">
        {t("settings.notes.skillPicker")}
      </span>
      <Dropdown
        className="w-[200px]"
        options={options}
        selectedValue={selected}
        onSelect={(value) => void updateSetting("note_skill_id", value || null)}
      />
    </div>
  );
};
