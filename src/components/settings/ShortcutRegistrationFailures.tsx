import { useCallback, useEffect, useState } from "react";
import { RefreshCw } from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { commands, type RegistrationFailure } from "@/bindings";
import { formatKeyCombination } from "@/lib/utils/keyboard";
import { useOsType } from "@/hooks/useOsType";
import { useSettings } from "@/hooks/useSettings";
import { Button } from "../ui/Button";

export const ShortcutRegistrationFailures = () => {
  const { t } = useTranslation();
  const osType = useOsType();
  const { getSetting } = useSettings();
  const bindings = getSetting("bindings") ?? {};
  const [failures, setFailures] = useState<RegistrationFailure[]>([]);
  const [isRetrying, setIsRetrying] = useState(false);

  const refreshFailures = useCallback(async () => {
    try {
      setFailures(await commands.getShortcutRegistrationFailures());
    } catch (error) {
      console.error("Failed to load shortcut registration failures:", error);
    }
  }, []);

  useEffect(() => {
    let stopListening: (() => void) | undefined;
    let cancelled = false;

    const subscribeAndRefresh = async () => {
      const unlisten = await listen<RegistrationFailure[]>(
        "shortcut-registration-failures-changed",
        (event) => setFailures(event.payload),
      );
      if (cancelled) {
        unlisten();
        return;
      }
      stopListening = unlisten;
      await refreshFailures();
    };

    void subscribeAndRefresh();
    return () => {
      cancelled = true;
      stopListening?.();
    };
  }, [bindings, refreshFailures]);

  const retry = async () => {
    setIsRetrying(true);
    try {
      setFailures(await commands.retryShortcutRegistrations());
    } catch (error) {
      console.error("Failed to retry shortcut registration:", error);
      toast.error(t("settings.general.shortcut.failurePanel.retryError"));
    } finally {
      setIsRetrying(false);
    }
  };

  if (failures.length === 0) {
    return null;
  }

  return (
    <section className="rounded-lg border border-red-500/30 bg-red-500/10 p-4">
      <div className="flex items-start justify-between gap-4">
        <div>
          <h3 className="text-sm font-semibold text-red-400">
            {t("settings.general.shortcut.failurePanel.title")}
          </h3>
          <p className="mt-1 text-xs text-mid-gray">
            {t("settings.general.shortcut.failurePanel.description")}
          </p>
        </div>
        <Button
          type="button"
          variant="secondary"
          size="sm"
          disabled={isRetrying}
          onClick={() => void retry()}
        >
          <RefreshCw
            className={"h-3.5 w-3.5 " + (isRetrying ? "animate-spin" : "")}
          />
          {t(
            isRetrying
              ? "settings.general.shortcut.failurePanel.retrying"
              : "settings.general.shortcut.failurePanel.retry",
          )}
        </Button>
      </div>
      <ul className="mt-3 space-y-2">
        {failures.map((failure) => {
          const storedBinding = bindings[failure.id];
          const bindingName = t(
            "settings.general.shortcut.bindings." + failure.id + ".name",
            storedBinding?.name ?? failure.id,
          );
          return (
            <li
              key={failure.id}
              className="rounded-md border border-red-500/20 bg-background/60 px-3 py-2"
            >
              <div className="flex flex-wrap items-center gap-2">
                <span className="text-sm font-medium">{bindingName}</span>
                <kbd className="rounded border border-mid-gray/30 bg-mid-gray/10 px-1.5 py-0.5 text-xs font-semibold">
                  {formatKeyCombination(failure.binding, osType)}
                </kbd>
              </div>
              <p className="mt-1 break-words text-xs text-red-400">
                {failure.error}
              </p>
            </li>
          );
        })}
      </ul>
    </section>
  );
};
