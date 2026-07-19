import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { Save } from "lucide-react";

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
        className="flex items-center gap-1 text-xs px-2 py-1 rounded-md border border-zinc-700 text-text/60 hover:text-text hover:border-blue-500 disabled:opacity-40 disabled:cursor-not-allowed transition-colors cursor-pointer"
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
        className="rounded-md border border-zinc-700 bg-zinc-800 px-2 py-1 text-xs text-zinc-100 placeholder-zinc-500 w-32 focus:border-blue-500 focus:outline-none"
      />
      <button
        type="button"
        onClick={confirm}
        className="text-xs px-2 py-1 rounded-md bg-blue-600 text-white hover:bg-blue-500 cursor-pointer"
      >
        {t("modelTesting.library.confirm")}
      </button>
      <button
        type="button"
        onClick={() => {
          setEditing(false);
          setName("");
        }}
        className="text-xs px-2 py-1 rounded-md border border-zinc-700 text-text/60 hover:text-text cursor-pointer"
      >
        {t("modelTesting.library.cancel")}
      </button>
    </span>
  );
};
