/** Hosted Riva functions offered as presets next to the NVIDIA Function ID. */
export const NVIDIA_FUNCTION_PRESETS = [
  {
    key: "whisper",
    id: "b702f636-f60c-4a3d-a6f4-f3568c13bd7d",
  },
  {
    key: "parakeet",
    id: "71203149-d3b7-4460-8231-1be2543a1fca",
  },
] as const;

export type NvidiaPresetKey = (typeof NVIDIA_FUNCTION_PRESETS)[number]["key"];

/** The preset whose Function ID equals `functionId`, or "custom". */
export const presetKeyForFunctionId = (
  functionId: string,
): NvidiaPresetKey | "custom" => {
  const id = functionId.trim().toLowerCase();
  return NVIDIA_FUNCTION_PRESETS.find((p) => p.id === id)?.key ?? "custom";
};
