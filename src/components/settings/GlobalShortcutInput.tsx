import React, { useEffect, useState, useRef } from "react";
import { useTranslation } from "react-i18next";
import {
  getKeyName,
  formatKeyCombination,
  normalizeKey,
  normalizeChord,
} from "../../lib/utils/keyboard";
import { X } from "lucide-react";
import { ResetButton } from "../ui/ResetButton";
import { AltGrWarning } from "./AltGrWarning";
import { ShortcutConflictWarning } from "./ShortcutConflictWarning";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";
import { commands } from "@/bindings";
import { toast } from "sonner";

interface GlobalShortcutInputProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  shortcutId: string;
  disabled?: boolean;
}

export const GlobalShortcutInput: React.FC<GlobalShortcutInputProps> = ({
  descriptionMode = "tooltip",
  grouped = false,
  shortcutId,
  disabled = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateBinding, resetBinding, isUpdating, isLoading } =
    useSettings();
  const [keyPressed, setKeyPressed] = useState<string[]>([]);
  const [recordedKeys, setRecordedKeys] = useState<string[]>([]);
  const [editingShortcutId, setEditingShortcutId] = useState<string | null>(
    null,
  );
  const [originalBinding, setOriginalBinding] = useState<string>("");
  const shortcutRefs = useRef<Map<string, HTMLDivElement | null>>(new Map());
  // True between suspendAllBindings and resumeAllBindings for this editor.
  const suspendedRef = useRef(false);
  const osType = useOsType();

  const bindings = getSetting("bindings") || {};

  // Every shortcut is off while a chord is recorded (see startRecording). Turn
  // them back on; returns what failed to register.
  const resumeShortcuts = async () => {
    if (!suspendedRef.current) return [];
    suspendedRef.current = false;
    return commands.resumeAllBindings().catch((error) => {
      console.error("Failed to resume shortcuts:", error);
      return [];
    });
  };

  // Leaving the page mid-recording must not leave every shortcut switched off.
  useEffect(
    () => () => {
      if (suspendedRef.current) {
        suspendedRef.current = false;
        commands.resumeAllBindings().catch(console.error);
      }
    },
    [],
  );

  useEffect(() => {
    // Only add event listeners when we're in editing mode
    if (editingShortcutId === null) return;

    let cleanup = false;

    // Keyboard event listeners. Escape is recorded like any other key (it is
    // the Cancel default); clicking anywhere else stops editing.
    const handleKeyDown = async (e: KeyboardEvent) => {
      if (cleanup) return;
      if (e.repeat) return; // ignore auto-repeat
      e.preventDefault();

      // Get the key with OS-specific naming and normalize it
      const rawKey = getKeyName(e, osType);
      const key = normalizeKey(rawKey);

      if (!keyPressed.includes(key)) {
        setKeyPressed((prev) => [...prev, key]);
        // Also add to recorded keys if not already there
        if (!recordedKeys.includes(key)) {
          setRecordedKeys((prev) => [...prev, key]);
        }
      }
    };

    const handleKeyUp = async (e: KeyboardEvent) => {
      if (cleanup) return;
      e.preventDefault();

      // Get the key with OS-specific naming and normalize it
      const rawKey = getKeyName(e, osType);
      const key = normalizeKey(rawKey);

      // Remove from currently pressed keys
      setKeyPressed((prev) => prev.filter((k) => k !== key));

      // If no keys are pressed anymore, commit the shortcut
      const updatedKeyPressed = keyPressed.filter((k) => k !== key);
      if (updatedKeyPressed.length === 0 && recordedKeys.length > 0) {
        // Create the shortcut string from all recorded keys
        // Sort keys so modifiers come first, then the main key
        const modifiers = [
          "ctrl",
          "control",
          "shift",
          "alt",
          "option",
          "meta",
          "command",
          "cmd",
          "super",
          "win",
          "windows",
        ];
        const sortedKeys = recordedKeys.sort((a, b) => {
          const aIsModifier = modifiers.includes(a.toLowerCase());
          const bIsModifier = modifiers.includes(b.toLowerCase());
          if (aIsModifier && !bIsModifier) return -1;
          if (!aIsModifier && bIsModifier) return 1;
          return 0;
        });
        const newShortcut = sortedKeys.join("+");

        if (editingShortcutId && bindings[editingShortcutId]) {
          let saved = false;
          try {
            await updateBinding(editingShortcutId, newShortcut);
            saved = true;
          } catch (error) {
            console.error("Failed to change binding:", error);
            toast.error(
              t("settings.general.shortcut.errors.set", {
                error: String(error),
              }),
            );

            // Reset to original binding on error
            if (originalBinding) {
              try {
                await updateBinding(editingShortcutId, originalBinding);
              } catch (resetError) {
                console.error("Failed to reset binding:", resetError);
                toast.error(t("settings.general.shortcut.errors.reset"));
              }
            }
          }

          // Registration happens when the shortcuts come back on. A chord another
          // app holds is reported here; a chord another shortcut holds is not —
          // duplicates are allowed and flagged by their conflict warning.
          const failures = await resumeShortcuts();
          const failure = failures.find((f) => f.id === editingShortcutId);
          const sharedWithAnother = Object.entries(bindings).some(
            ([id, b]) =>
              id !== editingShortcutId &&
              normalizeChord(b?.current_binding ?? "") ===
                normalizeChord(newShortcut),
          );
          if (saved && failure && !sharedWithAnother) {
            toast.error(
              t("settings.general.shortcut.errors.set", {
                error: failure.error,
              }),
            );
          }

          // Exit editing mode and reset states
          setEditingShortcutId(null);
          setKeyPressed([]);
          setRecordedKeys([]);
          setOriginalBinding("");
        }
      }
    };

    // Add click outside handler
    const handleClickOutside = async (e: MouseEvent) => {
      if (cleanup) return;
      const activeElement = shortcutRefs.current.get(editingShortcutId);
      if (activeElement && !activeElement.contains(e.target as Node)) {
        // Cancel shortcut recording and restore original binding
        if (editingShortcutId && originalBinding) {
          try {
            await updateBinding(editingShortcutId, originalBinding);
          } catch (error) {
            console.error("Failed to restore original binding:", error);
            toast.error(t("settings.general.shortcut.errors.restore"));
          }
        }
        await resumeShortcuts();
        setEditingShortcutId(null);
        setKeyPressed([]);
        setRecordedKeys([]);
        setOriginalBinding("");
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("keyup", handleKeyUp);
    window.addEventListener("click", handleClickOutside);

    return () => {
      cleanup = true;
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("keyup", handleKeyUp);
      window.removeEventListener("click", handleClickOutside);
    };
  }, [
    keyPressed,
    recordedKeys,
    editingShortcutId,
    bindings,
    originalBinding,
    updateBinding,
    osType,
  ]);

  // Start recording a new shortcut
  const startRecording = async (id: string) => {
    if (editingShortcutId === id) return; // Already editing this shortcut

    // Switch every shortcut off while recording: a chord another shortcut holds
    // would otherwise fire that shortcut instead of reaching this recorder.
    if (!suspendedRef.current) {
      suspendedRef.current = true;
      await commands.suspendAllBindings().catch(console.error);
    }

    // Store the original binding to restore if canceled
    setOriginalBinding(bindings[id]?.current_binding || "");
    setEditingShortcutId(id);
    setKeyPressed([]);
    setRecordedKeys([]);
  };

  // Format the current shortcut keys being recorded
  const formatCurrentKeys = (): string => {
    if (recordedKeys.length === 0)
      return t("settings.general.shortcut.pressKeys");

    // Use the same formatting as the display to ensure consistency
    return formatKeyCombination(recordedKeys.join("+"), osType);
  };

  // Store references to shortcut elements
  const setShortcutRef = (id: string, ref: HTMLDivElement | null) => {
    shortcutRefs.current.set(id, ref);
  };

  // Switch the shortcut off ("None"). An empty chord is never registered.
  const clearBinding = async () => {
    try {
      await updateBinding(shortcutId, "");
    } catch (error) {
      toast.error(
        t("settings.general.shortcut.errors.set", { error: String(error) }),
      );
    }
  };

  // If still loading, show loading state
  if (isLoading) {
    return (
      <SettingContainer
        title={t("settings.general.shortcut.title")}
        description={t("settings.general.shortcut.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <div className="text-sm text-mid-gray">
          {t("settings.general.shortcut.loading")}
        </div>
      </SettingContainer>
    );
  }

  // If no bindings are loaded, show empty state
  if (Object.keys(bindings).length === 0) {
    return (
      <SettingContainer
        title={t("settings.general.shortcut.title")}
        description={t("settings.general.shortcut.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <div className="text-sm text-mid-gray">
          {t("settings.general.shortcut.none")}
        </div>
      </SettingContainer>
    );
  }

  const binding = bindings[shortcutId];
  if (!binding) {
    return (
      <SettingContainer
        title={t("settings.general.shortcut.title")}
        description={t("settings.general.shortcut.notFound")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <div className="text-sm text-mid-gray">
          {t("settings.general.shortcut.none")}
        </div>
      </SettingContainer>
    );
  }

  // Get translated name and description for the binding
  const translatedName = t(
    `settings.general.shortcut.bindings.${shortcutId}.name`,
    binding.name,
  );
  const translatedDescription = t(
    `settings.general.shortcut.bindings.${shortcutId}.description`,
    binding.description,
  );

  return (
    <SettingContainer
      title={translatedName}
      description={translatedDescription}
      descriptionMode={descriptionMode}
      grouped={grouped}
      disabled={disabled}
      layout="horizontal"
    >
      <div className="flex items-center space-x-1">
        <ShortcutConflictWarning shortcutId={shortcutId} />
        <AltGrWarning binding={binding.current_binding} />
        {editingShortcutId === shortcutId ? (
          <div
            ref={(ref) => setShortcutRef(shortcutId, ref)}
            className="px-2 py-1 text-sm font-semibold border border-logo-primary bg-logo-primary/30 rounded-md"
          >
            {formatCurrentKeys()}
          </div>
        ) : (
          <div
            className={`px-2 py-1 text-sm bg-mid-gray/10 border border-mid-gray/80 hover:bg-logo-primary/10 rounded-md cursor-pointer hover:border-logo-primary ${
              binding.current_binding ? "font-semibold" : "text-mid-gray"
            }`}
            onClick={() => startRecording(shortcutId)}
          >
            {binding.current_binding
              ? formatKeyCombination(binding.current_binding, osType)
              : t("settings.general.shortcut.unset")}
          </div>
        )}
        <ResetButton
          onClick={clearBinding}
          disabled={
            !binding.current_binding || isUpdating(`binding_${shortcutId}`)
          }
          ariaLabel={t("settings.general.shortcut.clear")}
        >
          <X className="h-4 w-4" />
        </ResetButton>
        <ResetButton
          onClick={() => resetBinding(shortcutId)}
          disabled={isUpdating(`binding_${shortcutId}`)}
        />
      </div>
    </SettingContainer>
  );
};
