import React from "react";
import { useTranslation } from "react-i18next";
import { ArrowRight } from "lucide-react";
import { Button } from "../../ui/Button";
import { useNavStore, type FeatureSetup } from "../../../stores/navStore";

/**
 * A setup offered at the top of the settings it walks through (General › App,
 * Jumper): a tinted row that stands out, whose button opens Setups and starts
 * it there.
 */
export const SetupPromo: React.FC<{
  setup: FeatureSetup;
  icon: React.ComponentType<{ className?: string }>;
  title: string;
  text: string;
}> = ({ setup, icon: Icon, title, text }) => {
  const { t } = useTranslation();
  const startSetup = useNavStore((s) => s.startSetup);
  return (
    <div
      // A block of its own inside a group (card-break: not one of its rows).
      className="card-break flex items-center gap-4 rounded-lg border border-accent/40 bg-accent-soft px-4 py-3"
      // Titled like a setting, so search and What's new can point at it.
      data-setting-title={title}
    >
      <span className="flex-none flex items-center justify-center w-9 h-9 rounded-lg bg-surface text-accent-text">
        <Icon className="w-[18px] h-[18px]" aria-hidden />
      </span>
      <div className="flex-1 min-w-0">
        <h3 className="text-sm font-semibold">{title}</h3>
        <p className="text-[13px] leading-[1.5] text-text-secondary">{text}</p>
      </div>
      <Button
        variant="primary"
        className="flex-none"
        onClick={() => startSetup(setup)}
      >
        {t("setup.promo.start")}
        <ArrowRight className="w-3.5 h-3.5 rtl:-scale-x-100" aria-hidden />
      </Button>
    </div>
  );
};
