import React from "react";
import { Keyboard } from "lucide-react";

interface ShortcutChipProps {
  /** The shortcut as displayed ("Ctrl + Alt + R"), or null when not set. */
  keys: string | null;
  /** Text for a shortcut that is not set. */
  unsetLabel: string;
  /** Waiting for keys to be pressed. */
  recording: boolean;
  /** While recording: the keys pressed so far ("" before any). */
  recordedKeys: string;
  /** While recording: what to show before any key is pressed. */
  pressKeysLabel: string;
  onStartRecording: () => void;
  /** Receives the recording element (the inputs watch clicks outside it). */
  recordingRef?: (el: HTMLDivElement | null) => void;
  /** Clear and reset buttons, shown inside the chip after a divider. */
  children?: React.ReactNode;
}

/** One key as a keycap. */
const KeyCap: React.FC<{ label: string }> = ({ label }) => (
  <kbd className="inline-flex items-center justify-center h-5 min-w-5 px-1.5 rounded-sm border border-border border-b-control-bottom bg-surface2 font-sans text-[11px] font-semibold leading-none text-text">
    {label}
  </kbd>
);

const keyCaps = (combination: string) =>
  combination
    .split(/\s*\+\s*(?=\S)/)
    .filter(Boolean)
    .map((key, i) => <KeyCap key={`${key}-${i}`} label={key} />);

/**
 * The redesign's shortcut chip: each key its own keycap (quicker to read than
 * "Ctrl + Shift + F10", and nothing to translate), clicking the keys records a
 * new shortcut - the chip turns cyan meanwhile - and clear/reset sit inside it.
 */
export const ShortcutChip: React.FC<ShortcutChipProps> = ({
  keys,
  unsetLabel,
  recording,
  recordedKeys,
  pressKeysLabel,
  onStartRecording,
  recordingRef,
  children,
}) => (
  <div
    className={`flex items-center h-8 ps-1 pe-0.5 rounded-md border bg-control ${
      recording
        ? "border-accent shadow-[0_0_0_1px_var(--color-accent)]"
        : "border-control-border border-b-control-bottom"
    }`}
  >
    {recording ? (
      <div
        ref={recordingRef}
        className="flex items-center gap-1.5 h-6 px-1.5 text-[13px] font-medium text-accent-text"
      >
        {recordedKeys ? (
          keyCaps(recordedKeys)
        ) : (
          <>
            <Keyboard className="w-3.5 h-3.5 shrink-0" aria-hidden />
            {pressKeysLabel}
          </>
        )}
      </div>
    ) : (
      <div
        role="button"
        tabIndex={0}
        onClick={onStartRecording}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            onStartRecording();
          }
        }}
        className="flex items-center gap-1 h-6 px-1 rounded-sm cursor-pointer hover:bg-hover"
      >
        {keys ? (
          keyCaps(keys)
        ) : (
          <span className="px-1 text-[13px] text-text-secondary">
            {unsetLabel}
          </span>
        )}
      </div>
    )}
    {children && (
      <>
        <span className="mx-1 h-4 w-px bg-border" aria-hidden />
        {children}
      </>
    )}
  </div>
);
