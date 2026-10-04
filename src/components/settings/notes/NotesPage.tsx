import React from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "@/hooks/useSettings";
import { useNavStore, type NotesTab } from "@/stores/navStore";
import { noteSetupProblem } from "@/stores/notesStore";
import { STICKY_TABS, TabBar } from "../../ui/TabBar";
import { ManualNote } from "./ManualNote";
import { NoteSettingsTab } from "./NoteSettingsTab";
import { SavedNotes } from "./SavedNotes";

/** The Notes page: Settings, Saved notes, Manual note. */
export const NotesPage: React.FC = () => {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const picked = useNavStore((state) => state.notesTab);
  const setTab = useNavStore((state) => state.setNotesTab);

  // Until a tab is picked: Settings while notes can't be written yet,
  // otherwise the saved notes.
  const tab: NotesTab =
    picked ?? (noteSetupProblem(settings) ? "settings" : "saved");

  return (
    <div className="w-full space-y-4">
      <div className={STICKY_TABS}>
        <TabBar<NotesTab>
          tabs={[
            { id: "settings", label: t("settings.notes.tabs.settings") },
            { id: "saved", label: t("settings.notes.tabs.saved") },
            { id: "manual", label: t("settings.notes.tabs.manual") },
          ]}
          active={tab}
          onSelect={setTab}
        />
      </div>
      {tab === "settings" && <NoteSettingsTab />}
      {tab === "saved" && <SavedNotes />}
      {tab === "manual" && <ManualNote />}
    </div>
  );
};
