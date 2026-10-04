import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronDown } from "lucide-react";
import { useSettings } from "@/hooks/useSettings";
import { useNotesStore } from "@/stores/notesStore";
import { toggleSkillIds } from "./NoteSettingsTab";

/**
 * A compact switch for the skills notes are written with, for History's
 * toolbar: a button that says which skills are on ("2 skills") and opens a
 * small menu of checkboxes. Hidden until a skill is imported.
 */
export const NoteSkillPicker: React.FC = () => {
  const { t } = useTranslation();
  const { settings, updateSetting } = useSettings();
  const skills = useNotesStore((state) => state.skills);
  const loadSkills = useNotesStore((state) => state.loadSkills);
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    void loadSkills();
  }, [loadSkills]);

  useEffect(() => {
    if (!open) return;
    const close = (event: MouseEvent) => {
      if (ref.current && !ref.current.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, [open]);

  if (skills.length === 0) return null;

  const activeIds = (settings?.note_skill_ids ?? []).filter((id) =>
    skills.some((skill) => skill.id === id),
  );
  const summary =
    activeIds.length === 0
      ? t("settings.notes.skillPickerNone")
      : activeIds.length === 1
        ? (skills.find((skill) => skill.id === activeIds[0])?.name ?? "")
        : t("settings.notes.skillPickerCount", { count: activeIds.length });

  return (
    <div
      ref={ref}
      className="relative flex items-center gap-2 min-w-0"
      onKeyDown={(e) => {
        if (e.key === "Escape" && open) {
          e.stopPropagation();
          setOpen(false);
        }
      }}
    >
      <span className="text-xs text-text-secondary shrink-0">
        {t("settings.notes.skillPicker")}
      </span>
      <button
        type="button"
        aria-haspopup="true"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
        className={`h-8 ps-2.5 pe-2 w-[200px] text-sm text-text bg-control border border-control-border border-b-control-bottom rounded-md text-start flex items-center justify-between gap-2 hover:bg-control-hover cursor-pointer transition-colors duration-150 ${
          open ? "shadow-[inset_0_-2px_0_var(--color-accent)]" : ""
        }`}
      >
        <span className="truncate">{summary}</span>
        <ChevronDown
          className={`w-3.5 h-3.5 shrink-0 text-text-secondary transition-transform duration-150 ${open ? "rotate-180" : ""}`}
        />
      </button>
      {open && (
        <div className="absolute top-full end-0 mt-1 w-[260px] p-1 bg-surface border border-border rounded-lg shadow-float z-50 max-h-64 overflow-y-auto">
          {skills.map((skill) => (
            <label
              key={skill.id}
              className="flex items-center gap-2 min-h-8 px-2 rounded-md text-sm hover:bg-hover cursor-pointer"
            >
              <input
                type="checkbox"
                checked={activeIds.includes(skill.id)}
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
                className="w-4 h-4 shrink-0 accent-accent cursor-pointer"
              />
              <span className="truncate" title={skill.name}>
                {skill.name}
              </span>
            </label>
          ))}
        </div>
      )}
    </div>
  );
};
