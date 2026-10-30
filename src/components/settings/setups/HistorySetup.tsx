import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import {
  CalendarClock,
  Captions,
  Check,
  Copy,
  GripVertical,
  ListChecks,
  MoreHorizontal,
  Search,
  Star,
  Tag,
  Timer,
  Trash2,
  Users,
} from "lucide-react";
import { SetupFrame } from "./SetupFrame";
import { CombineSection } from "../combine/CombineSection";
import { useNavStore } from "../../../stores/navStore";

type Step = "intro" | "find" | "name" | "menu" | "combine" | "settings";
const STEPS: Step[] = ["intro", "find", "name", "menu", "combine", "settings"];

/** One line of a step: an icon tile and what that control does. */
const Tip: React.FC<{
  icon: React.ComponentType<{ className?: string }>;
  title: string;
  text: string;
}> = ({ icon: Icon, title, text }) => (
  <li className="flex items-start gap-3">
    <span className="flex-none flex items-center justify-center w-8 h-8 rounded-lg bg-accent-soft text-accent-text">
      <Icon className="w-4 h-4" aria-hidden />
    </span>
    <span className="min-w-0">
      <span className="block text-sm font-semibold">{title}</span>
      <span className="block max-w-[560px] text-[13px] leading-[1.5] text-text-secondary">
        {text}
      </span>
    </span>
  </li>
);

/**
 * Show me how: History (2.5.0). A tour, not a setup: finding a recording
 * (search, favorites, date and length), naming one, the ⋯ menu (again, with
 * speakers, delete), combining several into one text, and the Combine
 * settings, which are the real controls. Ends on History itself.
 */
export const HistorySetup: React.FC<{ onClose: () => void }> = ({
  onClose,
}) => {
  const { t } = useTranslation();
  const navigateTo = useNavStore((s) => s.navigateTo);
  const [step, setStep] = useState<Step>("intro");
  const index = STEPS.indexOf(step);
  const last = index === STEPS.length - 1;
  const next = () => {
    if (!last) {
      setStep(STEPS[index + 1]);
      return;
    }
    onClose();
    navigateTo("history", "recordings");
  };
  const k = (key: string) => t(`setup.history.${step}.${key}`);
  const tip = (
    icon: React.ComponentType<{ className?: string }>,
    key: string,
  ) => (
    <Tip
      key={key}
      icon={icon}
      title={t(`setup.history.${step}.tips.${key}.title`)}
      text={t(`setup.history.${step}.tips.${key}.text`)}
    />
  );

  const tips: Record<Step, React.ReactNode> = {
    intro: [
      tip(Search, "find"),
      tip(Tag, "name"),
      tip(MoreHorizontal, "menu"),
      tip(ListChecks, "combine"),
    ],
    find: [
      tip(Search, "search"),
      tip(Star, "favorites"),
      tip(CalendarClock, "date"),
      tip(Timer, "length"),
    ],
    name: [tip(Tag, "add"), tip(Check, "save")],
    menu: [
      tip(Copy, "icons"),
      tip(Captions, "again"),
      tip(Users, "speakers"),
      tip(Trash2, "delete"),
    ],
    combine: [
      tip(ListChecks, "tick"),
      tip(GripVertical, "order"),
      tip(Copy, "copy"),
    ],
    settings: null,
  };

  return (
    <SetupFrame
      name={t("setup.history.tourName")}
      step={index + 1}
      total={STEPS.length}
      heading={k("title")}
      body={k("body")}
      onClose={onClose}
      onBack={index > 0 ? () => setStep(STEPS[index - 1]) : undefined}
      primary={last ? t("setup.history.openHistory") : t("setup.next")}
      onPrimary={next}
    >
      {step === "settings" ? (
        <CombineSection />
      ) : (
        <ul className="flex flex-col gap-4">{tips[step]}</ul>
      )}
    </SetupFrame>
  );
};
