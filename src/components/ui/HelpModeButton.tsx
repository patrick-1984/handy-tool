import React from "react";
import { useTranslation } from "react-i18next";
import { CircleHelp } from "lucide-react";
import { useHelpMode } from "../../stores/helpModeStore";

/** The hint on a soft tint beside the button, the Esc key as a key cap. */
const Hint: React.FC<{ text: string }> = ({ text }) => {
  // The key as the language names it (French keyboards say Échap).
  const key = text.match(/Esc|Échap/)?.[0];
  const [before, after] = key ? text.split(key) : [text];
  return (
    <span className="inline-flex flex-wrap items-center gap-x-1 min-h-8 px-3 py-1 rounded-md bg-accent-soft text-[13px] text-text">
      {after === undefined ? (
        text
      ) : (
        <>
          {before}
          <kbd className="inline-flex items-center h-5 px-1.5 rounded border border-control-border border-b-control-bottom bg-surface font-sans text-xs font-semibold">
            {key}
          </kbd>
          {after}
        </>
      )}
    </span>
  );
};

/**
 * The ? at the top right of a page: turns help mode on. While it is on, the
 * first click anywhere (this button included) turns it off again - see
 * helpModeStore.
 */
export const HelpModeButton: React.FC = () => {
  const { t } = useTranslation();
  const on = useHelpMode((s) => s.on);
  const setOn = useHelpMode((s) => s.setOn);
  return (
    // Titled like a setting, so What's new can point at it.
    <div
      className="ms-auto flex items-center gap-3 rounded-lg"
      data-setting-title={t("helpMode.button")}
    >
      {on && <Hint text={t("helpMode.hint")} />}
      <button
        type="button"
        aria-pressed={on}
        aria-label={t("helpMode.button")}
        title={t("helpMode.button")}
        onClick={() => setOn(true)}
        className={`inline-flex items-center justify-center h-8 w-8 rounded-md transition-colors duration-150 cursor-pointer ${
          on
            ? "bg-btn text-on-btn"
            : "text-text-secondary hover:bg-hover hover:text-text"
        }`}
      >
        <CircleHelp className="w-[18px] h-[18px]" />
      </button>
    </div>
  );
};
