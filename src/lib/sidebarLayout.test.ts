import { describe, expect, test } from "bun:test";
import {
  MAX_PINS,
  moveBy,
  orderedEntries,
  resolveSidebar,
  togglePin,
} from "./sidebarLayout";
import { recordVisit, step, EMPTY_HISTORY } from "./navHistory";
import type { SidebarLayout } from "@/bindings";

const DEFAULTS = [
  "history",
  "general",
  "files",
  "jumper",
  "more-tools",
  "about",
  "more-settings",
];
const all = () => true;

describe("sidebar layout", () => {
  test("an empty layout is today's sidebar", () => {
    const r = resolveSidebar(null, DEFAULTS, all, all);
    expect(r.shown).toEqual(DEFAULTS);
    expect(r.more).toEqual([]);
    expect(r.isDefault).toBe(true);
  });

  test("unknown ids are dropped, new ones keep their default place", () => {
    expect(
      orderedEntries(DEFAULTS, ["about", "gone", "history", "files"]),
    ).toEqual([
      "about",
      "more-settings",
      "history",
      "general",
      "files",
      "jumper",
      "more-tools",
    ]);
  });

  test("entries under Show more, hidden pages and bad pins", () => {
    const layout: SidebarLayout = {
      order: [],
      in_more: ["about", "jumper"],
      shortcuts: [
        { id: "a", section: "mcp", icon: "zap" },
        {
          id: "b",
          section: "history",
          history_tab: "favorites",
          in_more: true,
        },
        { id: "c", section: "history", history_tab: "nonsense" },
        { id: "d", section: "general", history_tab: "favorites" },
        { id: "e", section: "debug" },
        { id: "f", section: "removedPage" },
      ],
    };
    const r = resolveSidebar(
      layout,
      DEFAULTS,
      (id) => id !== "debug",
      (id) => id !== "removedPage",
    );
    expect(r.more).toEqual(["jumper", "about"]);
    expect(r.shown).not.toContain("about");
    expect(r.pinsTop.map((s) => s.id)).toEqual(["a"]);
    expect(r.pinsMore.map((s) => s.id)).toEqual(["b"]);
    expect(r.isDefault).toBe(false);
  });

  test("pinning toggles and stops at the limit", () => {
    const pinned = togglePin(null, "history", "favorites")!;
    expect(pinned.shortcuts).toHaveLength(1);
    expect(pinned.shortcuts![0].icon).toBe("pin");
    // Another History tab is another page.
    expect(togglePin(pinned, "history", "statistics")!.shortcuts).toHaveLength(
      2,
    );
    expect(togglePin(pinned, "history", "favorites")!.shortcuts).toHaveLength(
      0,
    );
    const full: SidebarLayout = {
      shortcuts: Array.from({ length: MAX_PINS }, (_, i) => ({
        id: `s${i}`,
        section: `p${i}`,
      })),
    };
    expect(togglePin(full, "files")).toBeNull();
  });

  test("moving by one place stays inside the list", () => {
    expect(moveBy(["a", "b", "c"], 0, 1)).toEqual(["b", "a", "c"]);
    expect(moveBy(["a", "b", "c"], 0, -1)).toEqual(["a", "b", "c"]);
  });
});

describe("back and forward", () => {
  test("like a browser: back, forward, a new visit drops forward", () => {
    const a = { section: "general" };
    const b = { section: "files" };
    const c = { section: "history", historyTab: "favorites" };
    let h = recordVisit(EMPTY_HISTORY, a, b);
    h = recordVisit(h, b, c);
    const back = step(h, c, "back", all)!;
    expect(back.target).toEqual(b);
    const forward = step(back.history, b, "forward", all)!;
    expect(forward.target).toEqual(c);
    const again = recordVisit(back.history, b, a);
    expect(again.forward).toEqual([]);
  });

  test("pages switched off since are skipped", () => {
    let h = recordVisit(
      EMPTY_HISTORY,
      { section: "debug" },
      { section: "files" },
    );
    h = recordVisit(h, { section: "files" }, { section: "about" });
    const back = step(
      h,
      { section: "about" },
      "back",
      (t) => t.section !== "files",
    );
    expect(back!.target).toEqual({ section: "debug" });
    expect(step(EMPTY_HISTORY, { section: "about" }, "back", all)).toBeNull();
  });
});
