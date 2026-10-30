import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { toast } from "sonner";
import { Download, Users, X } from "lucide-react";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { SettingContainer } from "../../ui/SettingContainer";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { Dropdown } from "../../ui/Dropdown";
import { Button } from "../../ui/Button";
import { useSettings } from "../../../hooks/useSettings";
import {
  commands,
  type LocalSpeakersStatus,
  type SpeakersSource,
} from "@/bindings";

const SOURCES: SpeakersSource[] = ["auto", "local", "stt_server"];
const mb = (bytes: number) => Math.round(bytes / 1_000_000);

/** Advanced settings › Speakers: telling speakers apart on this PC (an
 *  optional download of NVIDIA's Sortformer, the model the STT server uses),
 *  and which one History's "Transcribe again with speakers" uses. */
export const SpeakersSection: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const [status, setStatus] = useState<LocalSpeakersStatus | null>(null);

  useEffect(() => {
    void commands.getLocalSpeakersStatus().then(setStatus);
    const unlisten = listen<LocalSpeakersStatus>("local-speakers-status", (e) =>
      setStatus(e.payload),
    );
    return () => {
      void unlisten.then((f) => f());
    };
  }, []);

  const download = async () => {
    setStatus((s) => (s ? { ...s, downloading: true } : s));
    const result = await commands.downloadLocalSpeakersModel();
    if (result.status === "error" && result.error !== "cancelled") {
      toast.error(
        t("settings.speakers.model.downloadFailed", { error: result.error }),
      );
    } else if (result.status === "ok") {
      toast.success(t("settings.speakers.model.ready"));
    }
    setStatus(await commands.getLocalSpeakersStatus());
  };

  const remove = async () => {
    if (!window.confirm(t("settings.speakers.model.deleteConfirm"))) return;
    const result = await commands.deleteLocalSpeakersModel();
    if (result.status === "error") toast.error(result.error);
    setStatus(await commands.getLocalSpeakersStatus());
  };

  const downloaded = status?.downloaded ?? false;
  const percent =
    status && status.size_bytes > 0
      ? Math.min(
          100,
          Math.round((status.downloaded_bytes / status.size_bytes) * 100),
        )
      : 0;
  const source = (getSetting("speakers_source") ?? "auto") as SpeakersSource;
  const afterTake = getSetting("local_speakers_after_take") ?? false;
  const pasteSpeakers = getSetting("local_speakers_paste") ?? false;

  return (
    <div className="space-y-4">
      <SettingsGroup
        icon={Users}
        title={t("settings.speakers.title")}
        description={t("settings.speakers.description")}
      >
        <div className="card-break flex flex-wrap items-center gap-4 rounded-lg border border-border bg-surface px-4 py-3">
          <div className="flex-1 min-w-[220px]">
            <h3 className="text-sm font-semibold">
              {t("settings.speakers.model.name")}
            </h3>
            <p className="text-[13px] leading-[1.5] text-text-secondary">
              {t("settings.speakers.model.description", {
                size: mb(status?.size_bytes ?? 492_268_840),
              })}{" "}
              <button
                type="button"
                className="text-accent-text underline cursor-pointer"
                onClick={() => status && void openUrl(status.license_url)}
              >
                {t("settings.speakers.model.license")}
              </button>
            </p>
            {status?.downloading && (
              <div className="mt-2 flex items-center gap-2">
                <div className="h-1.5 flex-1 rounded bg-control-bottom overflow-hidden">
                  <div
                    className="h-full bg-accent transition-[width]"
                    style={{ width: `${percent}%` }}
                  />
                </div>
                <span className="text-xs tabular-nums text-text-secondary">
                  {percent}%
                </span>
              </div>
            )}
          </div>
          {status?.downloading ? (
            <Button
              variant="secondary"
              onClick={() => void commands.cancelLocalSpeakersDownload()}
            >
              <X className="w-3.5 h-3.5" aria-hidden />
              {t("settings.speakers.model.cancel")}
            </Button>
          ) : downloaded ? (
            <span className="flex items-center gap-2">
              <span className="inline-flex items-center h-[22px] px-2 rounded bg-accent-soft text-accent-text text-xs font-semibold">
                {t("settings.speakers.model.downloaded")}
              </span>
              <Button variant="danger-ghost" onClick={() => void remove()}>
                {t("settings.speakers.model.delete")}
              </Button>
            </span>
          ) : (
            <Button variant="primary" onClick={() => void download()}>
              <Download className="w-3.5 h-3.5" aria-hidden />
              {t("settings.speakers.model.download", {
                size: mb(status?.size_bytes ?? 492_268_840),
              })}
            </Button>
          )}
        </div>
        <ToggleSwitch
          checked={afterTake}
          onChange={(v) => updateSetting("local_speakers_after_take", v)}
          isUpdating={isUpdating("local_speakers_after_take")}
          disabled={!downloaded}
          label={t("settings.speakers.afterTake.label")}
          description={t("settings.speakers.afterTake.description")}
          descriptionMode="tooltip"
          grouped={true}
        />
        <ToggleSwitch
          checked={pasteSpeakers}
          onChange={(v) => updateSetting("local_speakers_paste", v)}
          isUpdating={isUpdating("local_speakers_paste")}
          disabled={!downloaded || !afterTake}
          label={t("settings.speakers.paste.label")}
          description={t("settings.speakers.paste.description")}
          descriptionMode="tooltip"
          grouped={true}
        />
        <SettingContainer
          title={t("settings.speakers.source.title")}
          description={t("settings.speakers.source.description")}
          descriptionMode="tooltip"
          grouped={true}
        >
          <Dropdown
            options={SOURCES.map((value) => ({
              value,
              label: t(`settings.speakers.source.options.${value}`),
            }))}
            selectedValue={source}
            onSelect={(v) =>
              updateSetting("speakers_source", v as SpeakersSource)
            }
            disabled={isUpdating("speakers_source")}
          />
        </SettingContainer>
      </SettingsGroup>
    </div>
  );
};
