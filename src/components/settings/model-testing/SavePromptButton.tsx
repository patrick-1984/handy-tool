import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { Save } from "lucide-react";
import { TEXT_FIELD } from "../../ui/controlClasses";

interface Props {
  onSave: (name: string) => void;
  disabled?: boolean;
}

/** A "Save" button that expands inline to a name field + confirm/cancel. */
export const SavePromptButton: React.FC<Props> = ({ onSave, disabled }) => {
  const { t } = useTranslation();
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState("");

  const confirm = () => {
    const n = name.trim();
    if (!n) return;
    onSave(n);
    setName("");
    setEditing(false);
  };

  if (!editing) {
    return (
      <button
        type="button"
        disabled={disabled}
        onClick={() => setEditing(true)}
        title={t("modelTesting.library.save")}
        className="flex items-center gap-1 text-xs px-2 py-1 rounded-md border border-border text-text-secondary hover:text-text hover:border-accent disabled:opacity-40 disabled:cursor-not-allowed transition-colors cursor-pointer"
      >
        <Save className="w-3.5 h-3.5" />
        {t("modelTesting.library.save")}
      </button>
    );
  }

  return (
    <span className="flex items-center gap-1">
      <input
        autoFocus
        value={name}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") confirm();
          if (e.key === "Escape") {
            setEditing(false);
            setName("");
          }
        }}
        placeholder={t("modelTesting.library.namePlaceholder")}
        className={`${TEXT_FIELD} w-32`}
      />
      <button
        type="button"
        onClick={confirm}
        className="h-7 text-xs px-2.5 rounded-md bg-btn text-on-btn font-medium hover:bg-btn-hover cursor-pointer"
      >
        {t("modelTesting.library.confirm")}
      </button>
      <button
        type="button"
        onClick={() => {
          setEditing(false);
          setName("");
        }}
        className="text-xs px-2 py-1 rounded-md border border-border text-text-secondary hover:text-text cursor-pointer"
      >
        {t("modelTesting.library.cancel")}
      </button>
    </span>
  );
};
