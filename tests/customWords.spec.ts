import { test, expect } from "@playwright/test";
import {
  parseCustomWords,
  sanitizeCustomWord,
} from "../src/lib/utils/customWords";

test.describe("custom words parsing", () => {
  test("splits on spaces, tabs, new lines, commas and semicolons", () => {
    const { added, skipped } = parseCustomWords(
      "ChargeBee  Kubernetes\tOpenAI\nTauri\r\nWhisper,Handy; Vulkan",
      [],
    );
    expect(added).toEqual([
      "ChargeBee",
      "Kubernetes",
      "OpenAI",
      "Tauri",
      "Whisper",
      "Handy",
      "Vulkan",
    ]);
    expect(skipped).toBe(0);
  });

  test("skips words already in the list and repeats in the text", () => {
    const result = parseCustomWords("Handy Tauri Handy Rust Tauri", ["Rust"]);
    expect(result.added).toEqual(["Handy", "Tauri"]);
    expect(result.skipped).toBe(3);
    expect(result.duplicates).toEqual(["Handy", "Rust", "Tauri"]);
  });

  test("skips words that are too long or empty once cleaned", () => {
    const long = "a".repeat(51);
    const ok = "b".repeat(50);
    const result = parseCustomWords(`${long} ${ok} "" <&>`, []);
    expect(result.added).toEqual([ok]);
    expect(result.skipped).toBe(1);
    expect(result.duplicates).toEqual([]);
  });

  test("ignores Markdown list markup", () => {
    const result = parseCustomWords(
      "# Words\n- Kubernetes\n* `Tauri`\n1. **OpenAI**\n2) C# - Node.js",
      [],
    );
    expect(result.added).toEqual([
      "Words",
      "Kubernetes",
      "Tauri",
      "OpenAI",
      "C#",
      "Node.js",
    ]);
    expect(result.skipped).toBe(0);
  });

  test("keeps the old sanitizing rules", () => {
    expect(sanitizeCustomWord("  O'Brien ")).toBe("OBrien");
    expect(sanitizeCustomWord('<b>"x"&')).toBe("bx");
    expect(sanitizeCustomWord("two words")).toBeNull();
    expect(sanitizeCustomWord("   ")).toBeNull();
  });

  test("blank text adds and skips nothing", () => {
    expect(parseCustomWords(" \n\t ,; ", ["x"])).toEqual({
      added: [],
      skipped: 0,
      duplicates: [],
    });
  });
});
