import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { CircleCheck, Plus, RotateCw, Target } from "lucide-react";
import { SetupFrame, ChoiceCard } from "./SetupFrame";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { Button } from "../../ui/Button";
import { ShortcutInput } from "../ShortcutInput";
import { useSettings } from "../../../hooks/useSettings";
import { commands, type AnchorStatus } from "@/bindings";

type Goal = "jump" | "land" | "both";
type Step = "goal" | "mouse" | "places" | "shortcuts" | "test" | "done";
const STEPS: Step[] = ["goal", "mouse", "places", "shortcuts", "test", "done"];

/** Slots in the order places fill them: Hot 1, Hot 2, then slots 1-9. */
const SLOT_ORDER = [0, 10, 1, 2, 3, 4, 5, 6, 7, 8, 9];
/** A slot's Set and Jump shortcuts. */
const shortcutsOf = (slot: number): [string, string] =>
  slot === 0
    ? ["anchor_set", "anchor_jump"]
    : slot === 10
      ? ["anchor_set_2", "anchor_jump_2"]
      : [`jump_set_slot_${slot}`, `jump_slot_${slot}`];
const COUNTDOWN = 5;

/** Stands in for the app name in "Remembered: {{app}}", which is shown bold. */
const MARK = "⁣";

/** "Remembered: <b>app</b>" with a tick. */
const RememberedApp: React.FC<{ text: string; app: string }> = ({
  text,
  app,
}) => {
  const [before, after = ""] = text.split(MARK);
  return (
    <span className="flex items-center gap-1.5 min-w-0 text-[13px] text-text-secondary">
      <CircleCheck className="w-3.5 h-3.5 shrink-0 text-ok-text" aria-hidden />
      <span className="truncate">
        {before}
        <span className="font-semibold text-text">{app}</span>
        {after}
      </span>
    </span>
  );
};

/**
 * A place in the list: a target tile and its name, what it remembered and a
 * redo button at the end. While it is being shown (the countdown) the row is
 * tinted, the tile becomes its number in a ring, and the end says to click.
 */
const PlaceRow: React.FC<{
  label: string;
  number: number;
  counting: number | null;
  children?: React.ReactNode;
}> = ({ label, number, counting, children }) => {
  const { t } = useTranslation();
  return (
    <div
      className={`flex items-center gap-3 px-4 py-3 ${counting !== null ? "bg-accent-soft!" : ""}`}
    >
      {counting !== null ? (
        <span className="flex-none flex items-center justify-center w-7 h-7 rounded-full border-2 border-accent text-xs font-semibold tabular-nums">
          {number}
        </span>
      ) : (
        <span className="flex-none flex items-center justify-center w-7 h-7 rounded-md bg-surface2 text-text-secondary">
          <Target className="w-4 h-4" aria-hidden />
        </span>
      )}
      <span className="flex-1 text-sm font-semibold truncate">{label}</span>
      {counting !== null ? (
        <span className="text-[13px] tabular-nums" aria-live="polite">
          {t("setup.jumper.places.countdown", { seconds: counting })}
        </span>
      ) : (
        children
      )}
    </div>
  );
};

/**
 * The Jumper setup: what the Jumper is for, whether to remember the mouse too,
 * "show me the place" (a countdown, then the field you clicked into is
 * remembered - as many places as you like), their shortcuts, and a test. The
 * expert settings (remote delays, tracking, flow rules) stay on the Jumper page.
 */
export const JumperSetup: React.FC<{ onClose: () => void }> = ({ onClose }) => {
  const { t } = useTranslation();
  const { refreshSettings, updateSetting } = useSettings();
  const [step, setStep] = useState<Step>("goal");
  const [goal, setGoal] = useState<Goal>("both");
  const [mouse, setMouse] = useState(false);
  const [captured, setCaptured] = useState<number[]>([]);
  const [countdown, setCountdown] = useState<number | null>(null);
  // The slot being shown right now (during the countdown).
  const [capturing, setCapturing] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [slots, setSlots] = useState<(AnchorStatus | null)[]>([]);

  useEffect(() => {
    commands.getJumpSlots().then(setSlots);
    const unlisten = listen<(AnchorStatus | null)[]>("anchor-changed", (e) =>
      setSlots(e.payload),
    );
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  const index = STEPS.indexOf(step);
  const next = () => setStep(STEPS[index + 1]);
  const back = () => setStep(STEPS[index - 1]);
  const placeName = (slot: number) =>
    t("setup.jumper.place", { n: captured.indexOf(slot) + 1 });

  // A countdown, then the field in focus is remembered in `slot`.
  const capture = async (slot: number) => {
    setError(null);
    await commands.changeJumperSaveCursorSlot(slot, mouse);
    setCapturing(slot);
    setCountdown(COUNTDOWN);
    const timer = setInterval(
      () => setCountdown((c) => (c === null ? c : Math.max(1, c - 1))),
      1000,
    );
    const result = await commands.setJumpSlotAfter(slot, COUNTDOWN * 1000);
    clearInterval(timer);
    setCountdown(null);
    setCapturing(null);
    if (result.status === "ok") {
      setCaptured((c) => (c.includes(slot) ? c : [...c, slot]));
    } else {
      setError(result.error);
    }
  };
  // Another place: the next free slot, else the next one not set here.
  const nextSlot =
    SLOT_ORDER.find((s) => !captured.includes(s) && s !== 0 && !slots[s]) ??
    SLOT_ORDER.find((s) => !captured.includes(s));

  // Save & next on the goal: dictation lands in place 1 when the take ends.
  const saveGoal = async () => {
    if (goal !== "jump") {
      for (const key of ["output_stop", "submit_stop"]) {
        await commands.changeAnchorActionSetting(key, "jump");
        await commands.changeAnchorActionSlotSetting(key, 0);
      }
      await refreshSettings();
    }
    next();
  };
  // Places are kept across restarts.
  const savePlaces = () => {
    if (captured.length > 0) updateSetting("jumper_persist", true);
    next();
  };

  const common = {
    name: t("setup.jumper.name"),
    step: step === "done" ? 0 : index + 1,
    total: STEPS.length - 1,
    onClose,
    onBack: index > 0 && step !== "done" ? back : undefined,
  };

  switch (step) {
    case "goal":
      return (
        <SetupFrame
          {...common}
          heading={t("setup.jumper.goal.title")}
          body={t("setup.jumper.goal.body")}
          primary={t("setup.saveNext")}
          onPrimary={() => void saveGoal()}
          onSkip={next}
        >
          <div className="flex flex-col gap-2">
            {(["jump", "land", "both"] as Goal[]).map((g) => (
              <ChoiceCard
                key={g}
                selected={goal === g}
                title={t(`setup.jumper.goal.${g}.title`)}
                detail={t(`setup.jumper.goal.${g}.detail`)}
                onClick={() => setGoal(g)}
              />
            ))}
          </div>
        </SetupFrame>
      );
    case "mouse":
      return (
        <SetupFrame
          {...common}
          heading={t("setup.jumper.mouse.title")}
          body={t("setup.jumper.mouse.body")}
          primary={t("setup.saveNext")}
          onPrimary={next}
          onSkip={next}
        >
          <div className="flex flex-col gap-2">
            <ChoiceCard
              selected={!mouse}
              title={t("setup.jumper.mouse.no.title")}
              detail={t("setup.jumper.mouse.no.detail")}
              onClick={() => setMouse(false)}
            />
            <ChoiceCard
              selected={mouse}
              title={t("setup.jumper.mouse.yes.title")}
              detail={t("setup.jumper.mouse.yes.detail")}
              onClick={() => setMouse(true)}
            />
          </div>
        </SetupFrame>
      );
    case "places":
      return (
        <SetupFrame
          {...common}
          heading={t("setup.jumper.places.title")}
          body={t("setup.jumper.places.body", { seconds: COUNTDOWN })}
          primary={t("setup.saveNext")}
          onPrimary={savePlaces}
          onSkip={next}
          primaryDisabled={countdown !== null}
        >
          <SettingsGroup>
            {captured.map((slot) => (
              <PlaceRow
                key={slot}
                label={placeName(slot)}
                number={captured.indexOf(slot) + 1}
                counting={capturing === slot ? countdown : null}
              >
                {capturing !== slot && slots[slot] && (
                  <RememberedApp
                    text={t("setup.jumper.places.remembered", {
                      app: MARK,
                    })}
                    app={slots[slot]!.app}
                  />
                )}
                {capturing !== slot && (
                  <button
                    type="button"
                    title={t("setup.jumper.places.again")}
                    aria-label={t("setup.jumper.places.again")}
                    disabled={countdown !== null}
                    onClick={() => void capture(slot)}
                    className="inline-flex items-center justify-center h-7 w-7 rounded-md text-text-secondary hover:bg-hover hover:text-text disabled:opacity-50 cursor-pointer disabled:cursor-not-allowed"
                  >
                    <RotateCw className="w-3.5 h-3.5" />
                  </button>
                )}
              </PlaceRow>
            ))}
            {capturing !== null && !captured.includes(capturing) && (
              <PlaceRow
                label={t("setup.jumper.place", { n: captured.length + 1 })}
                number={captured.length + 1}
                counting={countdown}
              />
            )}
            {capturing === null && (
              <div className="flex items-center justify-between gap-3 px-4 py-3">
                <span className="text-sm text-text-secondary">
                  {captured.length === 0
                    ? t("setup.jumper.places.first")
                    : t("setup.jumper.places.another")}
                </span>
                {captured.length === 0 ? (
                  <Button variant="primary" onClick={() => void capture(0)}>
                    {t("setup.jumper.places.show")}
                  </Button>
                ) : (
                  nextSlot !== undefined && (
                    <Button
                      variant="secondary"
                      onClick={() => void capture(nextSlot)}
                    >
                      <Plus className="w-3.5 h-3.5" />
                      {t("setup.jumper.places.add")}
                    </Button>
                  )
                )}
              </div>
            )}
          </SettingsGroup>
          {error && (
            <p className="text-sm text-err-text">
              {t("setup.jumper.places.failed", { reason: error })}
            </p>
          )}
        </SetupFrame>
      );
    case "shortcuts": {
      const shown = captured.length > 0 ? captured : [0];
      return (
        <SetupFrame
          {...common}
          heading={t("setup.jumper.shortcuts.title")}
          body={t("setup.jumper.shortcuts.body")}
          primary={t("setup.saveNext")}
          onPrimary={next}
          onSkip={next}
        >
          {shown.map((slot) => {
            const [set, jump] = shortcutsOf(slot);
            return (
              <SettingsGroup
                key={slot}
                title={captured.length > 0 ? placeName(slot) : undefined}
              >
                <ShortcutInput shortcutId={set} grouped />
                <ShortcutInput shortcutId={jump} grouped />
              </SettingsGroup>
            );
          })}
        </SetupFrame>
      );
    }
    case "test":
      return (
        <SetupFrame
          {...common}
          heading={t("setup.jumper.test.title")}
          body={
            goal === "jump"
              ? t("setup.jumper.test.bodyJump")
              : t("setup.jumper.test.bodyLand")
          }
          primary={t("setup.next")}
          onPrimary={next}
        >
          {captured.length === 0 ? (
            <p className="text-sm text-text-secondary">
              {t("setup.jumper.test.none")}
            </p>
          ) : (
            <SettingsGroup>
              {captured.map((slot) => (
                <div
                  key={slot}
                  className="flex items-center justify-between gap-3 px-4 py-3"
                >
                  <span className="text-sm">
                    <span className="font-semibold">{placeName(slot)}</span>
                    {slots[slot] && (
                      <span className="text-text-secondary">
                        {" · "}
                        {slots[slot]!.app}
                      </span>
                    )}
                  </span>
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => void commands.jumpToSlot(slot)}
                  >
                    {t("setup.jumper.test.jump")}
                  </Button>
                </div>
              ))}
            </SettingsGroup>
          )}
        </SetupFrame>
      );
    case "done":
      return (
        <SetupFrame
          {...common}
          heading={t("setup.jumper.done.title")}
          body={t("setup.jumper.done.body")}
          primary={t("setup.finish")}
          onPrimary={onClose}
        />
      );
  }
};
