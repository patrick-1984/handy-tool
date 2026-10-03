import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { SetupFrame } from "./SetupFrame";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { ShortcutInput } from "../ShortcutInput";
import { useSettings } from "../../../hooks/useSettings";
import {
  KeeperStatus,
  ShortcutKeeperGroup,
  ShortcutKeeperProvider,
  useKeeperState,
} from "../shortcuts/ShortcutKeeper";

type Step = "intro" | "switch" | "shortcuts" | "try" | "done";
const STEPS: Step[] = ["intro", "switch", "shortcuts", "try", "done"];

/** The shortcuts used most inside a session; every one of them can be kept. */
const SHORTCUTS = [
  "transcribe",
  "transcribe_and_submit",
  "paste_last",
  "cancel",
];

/**
 * Remote Desktop setup (Windows only): what Shortcut Keeper does, switching
 * it on, ticking the shortcuts to keep on this PC, and trying it in a
 * session. Every control is the one on the Shortcuts page.
 */
export const RemoteDesktopSetup: React.FC<{ onClose: () => void }> = ({
  onClose,
}) => (
  <ShortcutKeeperProvider>
    <Steps onClose={onClose} />
  </ShortcutKeeperProvider>
);

const Steps: React.FC<{ onClose: () => void }> = ({ onClose }) => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const [step, setStep] = useState<Step>("intro");
  const index = STEPS.indexOf(step);
  const next = () => setStep(STEPS[index + 1]);
  const back = () => setStep(STEPS[index - 1]);
  const bindings = getSetting("bindings") ?? {};
  const state = useKeeperState();
  // On, and able to run here (not after the other keyboard backend).
  const on =
    (getSetting("remote_keys_enabled") ?? false) &&
    state !== "unavailable" &&
    state !== "needs_restart";

  const common = {
    name: t("setup.remote.name"),
    step: step === "done" ? 0 : index + 1,
    total: STEPS.length - 1,
    onClose,
    onBack: index > 0 && step !== "done" ? back : undefined,
  };

  switch (step) {
    case "intro":
      return (
        <SetupFrame
          {...common}
          heading={t("setup.remote.intro.title")}
          body={t("setup.remote.intro.body")}
          primary={t("setup.next")}
          onPrimary={next}
        />
      );
    case "switch":
      return (
        <SetupFrame
          {...common}
          heading={t("setup.remote.switch.title")}
          body={t("setup.remote.switch.body")}
          primary={t("setup.next")}
          onPrimary={next}
          primaryDisabled={!on}
        >
          <ShortcutKeeperGroup />
        </SetupFrame>
      );
    case "shortcuts":
      return (
        <SetupFrame
          {...common}
          heading={t("setup.remote.shortcuts.title")}
          body={t("setup.remote.shortcuts.body")}
          primary={t("setup.next")}
          onPrimary={next}
        >
          <SettingsGroup>
            {SHORTCUTS.filter((id) => bindings[id]).map((id) => (
              <ShortcutInput key={id} shortcutId={id} grouped />
            ))}
          </SettingsGroup>
          <p className="text-[13px] text-text-secondary">
            {t("setup.remote.shortcuts.more")}
          </p>
        </SetupFrame>
      );
    case "try":
      return (
        <SetupFrame
          {...common}
          heading={t("setup.remote.try.title")}
          body={t("setup.remote.try.body")}
          primary={t("setup.next")}
          onPrimary={next}
        >
          <SettingsGroup>
            <KeeperStatus />
          </SettingsGroup>
        </SetupFrame>
      );
    case "done":
      return (
        <SetupFrame
          {...common}
          heading={t("setup.remote.done.title")}
          body={t("setup.remote.done.body")}
          primary={t("setup.finish")}
          onPrimary={onClose}
        />
      );
  }
};
