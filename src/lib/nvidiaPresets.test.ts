import { describe, expect, test } from "bun:test";
import {
  NVIDIA_FUNCTION_PRESETS,
  presetKeyForFunctionId,
} from "./nvidiaPresets";

describe("nvidia presets", () => {
  test("Whisper is first (the default) and has the known ID", () => {
    expect(NVIDIA_FUNCTION_PRESETS[0].key).toBe("whisper");
    expect(NVIDIA_FUNCTION_PRESETS[0].id).toBe(
      "b702f636-f60c-4a3d-a6f4-f3568c13bd7d",
    );
  });
  test("known IDs map to their preset, ignoring case and spaces", () => {
    expect(presetKeyForFunctionId("b702f636-f60c-4a3d-a6f4-f3568c13bd7d")).toBe(
      "whisper",
    );
    expect(
      presetKeyForFunctionId(" 71203149-D3B7-4460-8231-1BE2543A1FCA "),
    ).toBe("parakeet");
  });
  test("anything else is custom", () => {
    expect(presetKeyForFunctionId("")).toBe("custom");
    expect(presetKeyForFunctionId("0000")).toBe("custom");
  });
});
