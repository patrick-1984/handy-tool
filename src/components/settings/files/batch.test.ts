import { describe, expect, test } from "bun:test";
import {
  combineBatch,
  dedupePaths,
  defaultOrder,
  resolveBatchOptions,
} from "./batch";
import type { AudioFileInfo } from "@/bindings";

const info = (name: string, created: number | null): AudioFileInfo => ({
  path: `C:\rec\${name}`,
  file_name: name,
  supported: true,
  created_ms: created,
  duration_secs: 1,
});

describe("several files", () => {
  test("the same file twice counts once, case-insensitively on Windows", () => {
    expect(dedupePaths(["C:\a.wav", "c:\A.WAV", "C:\b.wav"], true)).toEqual([
      "C:\a.wav",
      "C:\b.wav",
    ]);
    expect(dedupePaths(["/a.wav", "/A.wav"], false)).toEqual([
      "/a.wav",
      "/A.wav",
    ]);
  });

  test("oldest first, undated last, ties by name", () => {
    const order = defaultOrder([
      info("c.wav", null),
      info("b10.wav", 5),
      info("b9.wav", 5),
      info("a.wav", 9),
    ]).map((i) => i.file_name);
    expect(order).toEqual(["b9.wav", "b10.wav", "a.wav", "c.wav"]);
  });

  test("an empty line between parts by default", () => {
    const parts = [
      { fileName: "one.m4a", text: " First. " },
      { fileName: "two.m4a", text: "Second." },
    ];
    expect(combineBatch(parts, resolveBatchOptions(null))).toBe(
      "First.\n\nSecond.",
    );
  });

  test("numbers and names count only the parts with text", () => {
    const parts = [
      { fileName: "one.m4a", text: "First." },
      { fileName: "broken.m4a", text: null },
      { fileName: "three.m4a", text: "Third." },
    ];
    expect(
      combineBatch(parts, {
        separator: "divider",
        numbered: true,
        names: true,
      }),
    ).toBe("1. one.m4a\nFirst.\n\n---\n\n2. three.m4a\nThird.");
    expect(
      combineBatch(parts, { separator: "none", numbered: false, names: false }),
    ).toBe("First.\nThird.");
  });
});
