import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { ArrowRight, Mic, MoveUpRight, Palette, Sparkles } from "lucide-react";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { Button } from "../../ui/Button";
import { runSetupAgain } from "../../../lib/runSetup";
import { useOsType } from "../../../hooks/useOsType";
import { JumperSetup } from "./JumperSetup";
import { PostProcessingSetup } from "./PostProcessingSetup";
import { AppearanceSetup } from "./appearance/AppearanceSetup";
import { useNavStore, type FeatureSetup } from "../../../stores/navStore";

/**
 * Setups (sidebar, after General): the guided setups, each with what it covers.
 * The basic one is the first-start guide (full window); the feature setups run
 * here on the page, in place of the list.
 */
export const SetupsPage: React.FC = () => {
  const { t } = useTranslation();
  const osType = useOsType();
  const [running, setRunning] = useState<FeatureSetup | null>(null);
  const close = () => setRunning(null);

  // Started from a button elsewhere (General › App, Jumper, the first start).
  const pending = useNavStore((s) => s.pendingSetup);
  const clearPending = useNavStore((s) => s.clearPendingSetup);
  useEffect(() => {
    if (!pending) return;
    setRunning(pending);
    clearPending();
  }, [pending, clearPending]);

  if (running === "appearance") return <AppearanceSetup onClose={close} />;
  if (running === "jumper") return <JumperSetup onClose={close} />;
  if (running === "postProcessing")
    return <PostProcessingSetup onClose={close} />;

  // A row: an icon tile, the name (and a tag), what it covers, and Start.
  // Secondary buttons: one list of three primary ones would compete.
  const card = (
    key: string,
    Icon: React.ComponentType<{ className?: string }>,
    onStart: () => void,
    tag?: string,
  ) => {
    const title = t(`setup.catalog.${key}.title`);
    return (
      <div
        className="flex items-center gap-4 px-4 py-4"
        // Titled like a setting, so What's new can point at it.
        data-setting-title={title}
      >
        <span className="flex-none flex items-center justify-center w-9 h-9 rounded-lg bg-accent-soft text-accent-text">
          <Icon className="w-[18px] h-[18px]" aria-hidden />
        </span>
        <div className="flex-1 min-w-0 flex flex-col gap-0.5">
          <span className="flex items-center gap-2">
            <h3 className="text-sm font-semibold">{title}</h3>
            {tag && (
              <span className="inline-flex items-center h-5 px-1.5 rounded bg-surface2 border border-border text-xs text-text-secondary">
                {tag}
              </span>
            )}
          </span>
          <p className="max-w-[560px] text-[13px] leading-[1.5] text-text-secondary">
            {t(`setup.catalog.${key}.description`)}
          </p>
        </div>
        <Button variant="secondary" className="flex-none" onClick={onStart}>
          {t("setup.catalog.start")}
          <ArrowRight className="w-3.5 h-3.5 rtl:-scale-x-100" aria-hidden />
        </Button>
      </div>
    );
  };

  return (
    <div className="w-full space-y-4">
      <p className="text-sm text-text-secondary">
        {t("setup.catalog.description")}
      </p>
      <SettingsGroup>
        {card("basic", Mic, runSetupAgain)}
        {card("appearance", Palette, () => setRunning("appearance"))}
        {card("postProcessing", Sparkles, () => setRunning("postProcessing"))}
        {/* The Jumper is Windows-only. */}
        {osType === "windows" &&
          card(
            "jumper",
            MoveUpRight,
            () => setRunning("jumper"),
            t("setup.catalog.windowsOnly"),
          )}
      </SettingsGroup>
    </div>
  );
};
