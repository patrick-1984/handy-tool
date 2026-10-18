import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { AudioLines, Server, TextQuote, Users } from "lucide-react";
import { SettingsGroup, SubSettings } from "../../ui/SettingsGroup";
import { SettingContainer } from "../../ui/SettingContainer";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { Dropdown } from "../../ui/Dropdown";
import { Button } from "../../ui/Button";
import { TEXT_FIELD } from "../../ui/controlClasses";
import { AffixSettings } from "../AffixSettings";
import { useSettings } from "../../../hooks/useSettings";
import { useModelStore } from "../../../stores/modelStore";
import { commands, type SttServerInfo, type SttServerPaste } from "@/bindings";

export const STT_SERVER_MODEL_ID = "stt-server";

/** A text setting typed in place and saved when the field is left. */
const TextSetting: React.FC<{
  setting:
    | "stt_server_url"
    | "stt_server_key"
    | "stt_server_model"
    | "stt_server_speakers_model";
  title: string;
  description: string;
  placeholder?: string;
  password?: boolean;
  onTyped?: (value: string) => void;
}> = ({ setting, title, description, placeholder, password, onTyped }) => {
  const { getSetting, updateSetting } = useSettings();
  const saved = (getSetting(setting) as string) ?? "";
  const [value, setValue] = useState(saved);
  useEffect(() => setValue(saved), [saved]);
  return (
    <SettingContainer
      title={title}
      description={description}
      descriptionMode="tooltip"
      grouped={true}
    >
      <input
        type={password ? "password" : "text"}
        value={value}
        onChange={(e) => {
          setValue(e.target.value);
          onTyped?.(e.target.value);
        }}
        onBlur={() => {
          if (value !== saved) updateSetting(setting, value);
        }}
        placeholder={placeholder}
        className={`w-full ${TEXT_FIELD}`}
      />
    </SettingContainer>
  );
};

/** A model picker filled from the server's own list (after Test), or a text
 *  field while the list is not known. */
const ModelSetting: React.FC<{
  setting: "stt_server_model" | "stt_server_speakers_model";
  title: string;
  description: string;
  info: SttServerInfo | null;
}> = ({ setting, title, description, info }) => {
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const value = (getSetting(setting) as string) ?? "";
  if (!info || info.models.length === 0) {
    return (
      <TextSetting setting={setting} title={title} description={description} />
    );
  }
  const options = info.models.map((m) => ({
    value: m.id,
    label: m.description ? `${m.id} — ${m.description}` : m.id,
  }));
  if (value && !options.some((o) => o.value === value)) {
    options.unshift({ value, label: value });
  }
  return (
    <SettingContainer
      title={title}
      description={description}
      descriptionMode="tooltip"
      grouped={true}
    >
      <Dropdown
        options={options}
        selectedValue={value || null}
        onSelect={(v) => updateSetting(setting, v)}
        disabled={isUpdating(setting)}
        className="w-80 [&>button]:w-full"
      />
    </SettingContainer>
  );
};

/** Advanced settings › STT server: a speech-to-text server of your own
 *  (OpenAI-compatible), with or without telling the speakers apart. */
export const SttServerSection: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const currentModel = useModelStore((s) => s.currentModel);
  const selectModel = useModelStore((s) => s.selectModel);

  const savedUrl = (getSetting("stt_server_url") as string) ?? "";
  const savedKey = (getSetting("stt_server_key") as string) ?? "";
  const [typedUrl, setTypedUrl] = useState(savedUrl);
  const [typedKey, setTypedKey] = useState(savedKey);
  useEffect(() => setTypedUrl(savedUrl), [savedUrl]);
  useEffect(() => setTypedKey(savedKey), [savedKey]);

  const [info, setInfo] = useState<SttServerInfo | null>(null);
  const [testState, setTestState] = useState<
    | { kind: "idle" }
    | { kind: "busy" }
    | { kind: "ok" }
    | { kind: "error"; message: string }
  >({ kind: "idle" });

  const test = async (url = typedUrl, key = typedKey) => {
    setTestState({ kind: "busy" });
    const result = await commands.getSttServerInfo(url, key);
    if (result.status === "ok") {
      setInfo(result.data);
      setTestState({ kind: "ok" });
    } else {
      setInfo(null);
      setTestState({ kind: "error", message: result.error });
    }
  };

  // Fill the model lists from a server that is already set up.
  useEffect(() => {
    if (savedUrl.trim()) void test(savedUrl, savedKey);
    // Only on opening the page.
  }, []);

  const speakers = (getSetting("stt_server_speakers") as boolean) ?? false;
  const paste = ((getSetting("stt_server_paste") as SttServerPaste) ??
    "text") as SttServerPaste;
  const inUse = currentModel === STT_SERVER_MODEL_ID;

  return (
    <div className="space-y-4">
      <SettingsGroup
        icon={Server}
        title={t("settings.sttServer.server.title")}
        description={t("settings.sttServer.server.description")}
      >
        <TextSetting
          setting="stt_server_url"
          title={t("settings.sttServer.url.title")}
          description={t("settings.sttServer.url.description")}
          placeholder="http://192.168.1.10:8000/v1"
          onTyped={setTypedUrl}
        />
        <TextSetting
          setting="stt_server_key"
          title={t("settings.sttServer.key.title")}
          description={t("settings.sttServer.key.description")}
          placeholder={t("settings.sttServer.key.placeholder")}
          password={true}
          onTyped={setTypedKey}
        />
        <SettingContainer
          title={t("settings.sttServer.test.title")}
          description={t("settings.sttServer.test.description")}
          descriptionMode="tooltip"
          grouped={true}
        >
          <div className="flex flex-col items-end gap-1">
            <Button
              variant="secondary"
              onClick={() => void test()}
              disabled={testState.kind === "busy" || !typedUrl.trim()}
            >
              {testState.kind === "busy"
                ? t("settings.sttServer.test.busy")
                : t("settings.sttServer.test.button")}
            </Button>
            {testState.kind === "ok" && info && (
              <span className="text-xs text-text-secondary text-right">
                {t("settings.sttServer.test.ok", { count: info.models.length })}
                {info.backend ? ` · ${info.backend}` : ""}
              </span>
            )}
            {testState.kind === "error" && (
              <span className="text-xs text-err-text text-right break-all">
                {t("settings.sttServer.test.failed", {
                  error: testState.message,
                })}
              </span>
            )}
          </div>
        </SettingContainer>
        <SettingContainer
          title={t("settings.sttServer.use.title")}
          description={t("settings.sttServer.use.description")}
          descriptionMode="tooltip"
          grouped={true}
        >
          {inUse ? (
            <span className="text-sm text-text-secondary">
              {t("settings.sttServer.use.inUse")}
            </span>
          ) : (
            <Button
              variant="primary"
              onClick={() => void selectModel(STT_SERVER_MODEL_ID)}
              disabled={!savedUrl.trim()}
            >
              {t("settings.sttServer.use.button")}
            </Button>
          )}
        </SettingContainer>
      </SettingsGroup>

      <SettingsGroup
        icon={AudioLines}
        title={t("settings.sttServer.transcription.title")}
        description={t("settings.sttServer.transcription.description")}
      >
        <ModelSetting
          setting="stt_server_model"
          title={t("settings.sttServer.model.title")}
          description={t("settings.sttServer.model.description")}
          info={info}
        />
        <ToggleSwitch
          checked={speakers}
          onChange={(v) => updateSetting("stt_server_speakers", v)}
          isUpdating={isUpdating("stt_server_speakers")}
          label={t("settings.sttServer.speakers.label")}
          description={t("settings.sttServer.speakers.description")}
          descriptionMode="tooltip"
          grouped={true}
        />
        {speakers && (
          <SubSettings>
            <ModelSetting
              setting="stt_server_speakers_model"
              title={t("settings.sttServer.speakersModel.title")}
              description={t("settings.sttServer.speakersModel.description")}
              info={info}
            />
            <SettingContainer
              title={t("settings.sttServer.paste.title")}
              description={t("settings.sttServer.paste.description")}
              descriptionMode="tooltip"
              grouped={true}
            >
              <Dropdown
                options={(["text", "speakers"] as SttServerPaste[]).map(
                  (value) => ({
                    value,
                    label: t(`settings.sttServer.paste.options.${value}`),
                  }),
                )}
                selectedValue={paste}
                onSelect={(v) =>
                  updateSetting("stt_server_paste", v as SttServerPaste)
                }
                disabled={isUpdating("stt_server_paste")}
              />
            </SettingContainer>
          </SubSettings>
        )}
        <p className="px-1 text-xs text-text-secondary">
          {t("settings.sttServer.customWords")}
        </p>
      </SettingsGroup>

      <SettingsGroup
        icon={TextQuote}
        title={t("settings.sttServer.affixText.title")}
        description={t("settings.sttServer.affixText.description")}
      >
        <AffixSettings flow="stt_text" grouped={true} />
      </SettingsGroup>

      <SettingsGroup
        icon={Users}
        title={t("settings.sttServer.affixSpeakers.title")}
        description={t("settings.sttServer.affixSpeakers.description")}
      >
        <AffixSettings flow="stt_speakers" grouped={true} />
      </SettingsGroup>
    </div>
  );
};
