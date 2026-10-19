import type { TFunction } from "i18next";
import type { ModelInfo } from "@/bindings";
import type { SidebarSection } from "../../Sidebar";
import { highlightSetting } from "../../../lib/settingsSearch";

/** Where a model's own settings live: the page, and the setting to outline
 *  there (its title's i18n key). `null` = the model has nothing to set. */
export interface ModelConfigTarget {
  section: SidebarSection;
  titleKey: string;
}

/** Models whose language (and translation) the General page sets. */
const LANGUAGE_ENGINES: ModelInfo["engine_type"][] = [
  "Whisper",
  "SenseVoice",
  "FlmWhisper",
];

export const HF_SECTION_TITLE_KEY = "settings.models.huggingFace.title";

export const modelConfigTarget = (
  model: ModelInfo,
): ModelConfigTarget | null => {
  switch (model.engine_type) {
    case "ApiWhisper":
      return {
        section: "providers",
        titleKey: "settings.advanced.apiTranscription.url.title",
      };
    case "OpenRouterWhisper":
      return {
        section: "providers",
        titleKey: "settings.advanced.openRouterTranscription.url.title",
      };
    case "SttServer":
      return { section: "sttServer", titleKey: "settings.sttServer.url.title" };
    default:
      if (LANGUAGE_ENGINES.includes(model.engine_type)) {
        return {
          section: "general",
          titleKey: "settings.general.language.title",
        };
      }
      return null;
  }
};

/** Opens a model's settings and outlines them. */
export const openModelConfig = (
  target: ModelConfigTarget,
  navigateTo: (section: SidebarSection) => void,
  t: TFunction,
) => {
  navigateTo(target.section);
  highlightSetting(t(target.titleKey));
};
