import { test, expect } from "@playwright/test";
import { parseMarkdown } from "../src/components/settings/notes/markdown";

// The notes' Markdown renderer: nothing a model writes may go missing.
test.describe("note Markdown", () => {
  const listTexts = (markdown: string) =>
    parseMarkdown(markdown).flatMap((block) =>
      block.kind === "list" ? block.list.items.map((item) => item.text) : [],
    );

  test("a list that starts indented keeps every item", () => {
    expect(listTexts("  - a\n- b\n- c")).toEqual(["a", "b", "c"]);
    expect(listTexts("    - deep\n  - mid\n- top")).toEqual([
      "deep",
      "mid",
      "top",
    ]);
  });

  test("nested items stay under their parent", () => {
    const [block] = parseMarkdown("- a\n  - a1\n- b");
    expect(block.kind).toBe("list");
    if (block.kind !== "list") return;
    expect(block.list.items.map((item) => item.text)).toEqual(["a", "b"]);
    expect(block.list.items[0].children[0].items[0].text).toBe("a1");
  });

  test("an indented paragraph after a blank line stays in its item", () => {
    expect(listTexts("- a\n\n  more about a\n- b")).toEqual([
      "a more about a",
      "b",
    ]);
    expect(parseMarkdown("- a\n\nText after").map((b) => b.kind)).toEqual([
      "list",
      "paragraph",
    ]);
  });

  test("a table needs as many delimiter cells as header cells", () => {
    expect(parseMarkdown("| a | b |\n|---|---|\n| 1 | 2 |")[0].kind).toBe(
      "table",
    );
    expect(parseMarkdown("text with a|pipe\n---")[0]).toEqual({
      kind: "paragraph",
      text: "text with a|pipe",
    });
  });
});
