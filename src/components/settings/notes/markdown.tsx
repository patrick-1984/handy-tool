import React from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Square, SquareCheck } from "lucide-react";

/**
 * A small Markdown renderer for notes (the app has no Markdown library).
 * Covers what models write in notes: headings, paragraphs, bullet and
 * numbered lists (nested, with task boxes), block quotes, fenced code,
 * tables, rules, and inline bold, italic, strikethrough, code and links.
 * It builds React elements only - never HTML strings - so a note can't
 * inject markup.
 */

type Block =
  | { kind: "heading"; level: number; text: string }
  | { kind: "paragraph"; text: string }
  | { kind: "code"; text: string }
  | { kind: "rule" }
  | { kind: "quote"; blocks: Block[] }
  | { kind: "list"; list: ListNode }
  | { kind: "table"; header: string[]; rows: string[][] };

interface ListItem {
  text: string;
  /** null = not a task; true/false = a checked/unchecked task box. */
  task: boolean | null;
  children: ListNode[];
}

interface ListNode {
  ordered: boolean;
  start: number;
  items: ListItem[];
}

const LIST_ITEM = /^(\s*)([-*+]|\d{1,9}[.)])\s+(.*)$/;
const HEADING = /^(#{1,6})\s+(.*?)\s*#*\s*$/;
const RULE = /^\s{0,3}([-*_])(\s*\1){2,}\s*$/;
const FENCE = /^\s{0,3}(```|~~~)/;
const QUOTE = /^\s{0,3}>\s?(.*)$/;
const TABLE_SEPARATOR = /^\s*\|?\s*:?-+:?\s*(\|\s*:?-+:?\s*)*\|?\s*$/;

const isTableRow = (line: string) => line.includes("|");

const splitRow = (line: string): string[] =>
  line
    .trim()
    .replace(/^\|/, "")
    .replace(/\|$/, "")
    .split("|")
    .map((cell) => cell.trim());

/** A header row followed by a delimiter row with as many cells (GFM). */
const startsTable = (line: string, next: string | undefined) =>
  isTableRow(line) &&
  next !== undefined &&
  TABLE_SEPARATOR.test(next) &&
  splitRow(next).length === splitRow(line).length;

const startsBlock = (line: string, next: string | undefined) =>
  HEADING.test(line) ||
  RULE.test(line) ||
  FENCE.test(line) ||
  QUOTE.test(line) ||
  LIST_ITEM.test(line) ||
  startsTable(line, next);

/** Indent width with tabs counted as four spaces. */
const indentOf = (raw: string) => raw.replace(/\t/g, "    ").length;

const parseList = (lines: string[]): ListNode => {
  // Each entry: indent, marker, text. Continuation lines join the item above.
  const entries: { indent: number; marker: string; text: string }[] = [];
  for (const line of lines) {
    const match = LIST_ITEM.exec(line);
    if (match) {
      entries.push({
        indent: indentOf(match[1]),
        marker: match[2],
        text: match[3],
      });
    } else if (entries.length > 0) {
      entries[entries.length - 1].text += ` ${line.trim()}`;
    }
  }

  const build = (start: number, indent: number): [ListNode, number] => {
    const first = entries[start];
    const ordered = /\d/.test(first.marker);
    const node: ListNode = {
      ordered,
      start: ordered ? parseInt(first.marker, 10) : 1,
      items: [],
    };
    let i = start;
    while (i < entries.length) {
      const entry = entries[i];
      if (entry.indent < indent) break;
      if (entry.indent > indent) {
        // A deeper item: a nested list under the last item.
        const [child, next] = build(i, entry.indent);
        const parent = node.items[node.items.length - 1];
        if (parent) parent.children.push(child);
        else node.items.push({ text: "", task: null, children: [child] });
        i = next;
        continue;
      }
      const task = /^\[([ xX])\]\s+/.exec(entry.text);
      node.items.push({
        text: task ? entry.text.slice(task[0].length) : entry.text,
        task: task ? task[1] !== " " : null,
        children: [],
      });
      i++;
    }
    return [node, i];
  };

  // Items before the list's least-indented ones (a list that starts
  // indented) are not nested under anything: every run that stops at a
  // shallower item continues the same list.
  const [list, end] = build(0, entries[0].indent);
  let i = end;
  while (i < entries.length) {
    const [more, next] = build(i, entries[i].indent);
    list.items.push(...more.items);
    i = next;
  }
  return list;
};

export const parseMarkdown = (markdown: string): Block[] => {
  const lines = markdown.replace(/\r\n?/g, "\n").split("\n");
  const blocks: Block[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    if (line.trim() === "") {
      i++;
      continue;
    }

    const fence = FENCE.exec(line);
    if (fence) {
      const body: string[] = [];
      i++;
      while (i < lines.length && !lines[i].trim().startsWith(fence[1])) {
        body.push(lines[i]);
        i++;
      }
      i++; // the closing fence
      blocks.push({ kind: "code", text: body.join("\n") });
      continue;
    }

    const heading = HEADING.exec(line);
    if (heading) {
      blocks.push({
        kind: "heading",
        level: heading[1].length,
        text: heading[2],
      });
      i++;
      continue;
    }

    if (RULE.test(line)) {
      blocks.push({ kind: "rule" });
      i++;
      continue;
    }

    if (QUOTE.test(line)) {
      const inner: string[] = [];
      while (i < lines.length && QUOTE.test(lines[i])) {
        inner.push(QUOTE.exec(lines[i])![1]);
        i++;
      }
      blocks.push({ kind: "quote", blocks: parseMarkdown(inner.join("\n")) });
      continue;
    }

    if (startsTable(line, lines[i + 1])) {
      const header = splitRow(line);
      const rows: string[][] = [];
      i += 2;
      while (
        i < lines.length &&
        lines[i].trim() !== "" &&
        isTableRow(lines[i])
      ) {
        rows.push(splitRow(lines[i]));
        i++;
      }
      blocks.push({ kind: "table", header, rows });
      continue;
    }

    if (LIST_ITEM.test(line)) {
      const first = LIST_ITEM.exec(line)!;
      const baseIndent = indentOf(first[1]);
      const ordered = /\d/.test(first[2]);
      // An item at the list's own level with the other kind of marker
      // (bullets vs numbers) starts a new list.
      const otherKind = (candidate: string) => {
        const match = LIST_ITEM.exec(candidate);
        return (
          match !== null &&
          indentOf(match[1]) <= baseIndent &&
          /\d/.test(match[2]) !== ordered
        );
      };
      const listLines: string[] = [];
      while (i < lines.length) {
        const current = lines[i];
        if (current.trim() === "") {
          // A blank line inside a list continues it only if more items, or
          // an indented paragraph of the last item, follow.
          const next = lines[i + 1];
          const continues =
            next !== undefined &&
            ((LIST_ITEM.test(next) && !otherKind(next)) ||
              (!LIST_ITEM.test(next) &&
                next.trim() !== "" &&
                indentOf(/^\s*/.exec(next)![0]) > baseIndent &&
                !startsBlock(next.trim(), lines[i + 2])));
          if (continues) {
            i++;
            continue;
          }
          break;
        }
        if (otherKind(current)) break;
        if (!LIST_ITEM.test(current) && !/^\s/.test(current)) {
          if (startsBlock(current, lines[i + 1])) break;
          // A lazy continuation line of the last item.
        }
        listLines.push(current);
        i++;
      }
      blocks.push({ kind: "list", list: parseList(listLines) });
      continue;
    }

    const paragraph: string[] = [line.trim()];
    i++;
    while (
      i < lines.length &&
      lines[i].trim() !== "" &&
      !startsBlock(lines[i], lines[i + 1])
    ) {
      paragraph.push(lines[i].trim());
      i++;
    }
    blocks.push({ kind: "paragraph", text: paragraph.join("\n") });
  }

  return blocks;
};

const SAFE_LINK = /^(https?:|mailto:)/i;

/** Inline patterns, tried at each position; the earliest match wins. */
const INLINE =
  /(`+)([\s\S]*?[^`])\1(?!`)|\[([^\]]+)\]\(([^)\s]+)(?:\s+"[^"]*")?\)|\*\*([\s\S]+?)\*\*|__([\s\S]+?)__|~~([\s\S]+?)~~|\*([^*\s](?:[\s\S]*?[^*\s])?)\*|(?<![\w])_([^_\s](?:[\s\S]*?[^_\s])?)_(?![\w])|(https?:\/\/[^\s<>()]+[^\s<>().,;:!?'"])/;

const renderInline = (text: string, keyPrefix = "i"): React.ReactNode[] => {
  const nodes: React.ReactNode[] = [];
  let rest = text;
  let n = 0;
  while (rest.length > 0) {
    const match = INLINE.exec(rest);
    if (!match) {
      nodes.push(rest);
      break;
    }
    if (match.index > 0) nodes.push(rest.slice(0, match.index));
    const key = `${keyPrefix}-${n++}`;
    const [
      whole,
      ,
      code,
      linkText,
      linkUrl,
      bold1,
      bold2,
      strike,
      em1,
      em2,
      bare,
    ] = match;
    if (code !== undefined) {
      nodes.push(
        <code
          key={key}
          className="px-1 py-0.5 rounded bg-surface2 font-mono text-[0.85em]"
        >
          {code.trim()}
        </code>,
      );
    } else if (linkText !== undefined) {
      nodes.push(
        SAFE_LINK.test(linkUrl) ? (
          <NoteLink key={key} url={linkUrl}>
            {renderInline(linkText, key)}
          </NoteLink>
        ) : (
          <React.Fragment key={key}>
            {renderInline(linkText, key)}
          </React.Fragment>
        ),
      );
    } else if (bold1 !== undefined || bold2 !== undefined) {
      nodes.push(
        <strong key={key} className="font-semibold">
          {renderInline(bold1 ?? bold2, key)}
        </strong>,
      );
    } else if (strike !== undefined) {
      nodes.push(<s key={key}>{renderInline(strike, key)}</s>);
    } else if (em1 !== undefined || em2 !== undefined) {
      nodes.push(<em key={key}>{renderInline(em1 ?? em2, key)}</em>);
    } else if (bare !== undefined) {
      nodes.push(
        <NoteLink key={key} url={bare}>
          {bare}
        </NoteLink>,
      );
    } else {
      nodes.push(whole);
    }
    rest = rest.slice(match.index + whole.length);
  }
  return nodes;
};

/** Opens in the browser; the app window itself never navigates. */
const NoteLink: React.FC<{ url: string; children: React.ReactNode }> = ({
  url,
  children,
}) => (
  <a
    href={url}
    title={url}
    onClick={(event) => {
      event.preventDefault();
      void openUrl(url);
    }}
    className="text-accent-text underline underline-offset-2 hover:text-accent cursor-pointer"
  >
    {children}
  </a>
);

/** Inline text with single line breaks kept. */
const renderLines = (text: string, key: string) =>
  text.split("\n").map((line, i) => (
    <React.Fragment key={`${key}-l${i}`}>
      {i > 0 && <br />}
      {renderInline(line, `${key}-l${i}`)}
    </React.Fragment>
  ));

const HEADING_CLASSES = [
  "text-base font-semibold",
  "text-[15px] font-semibold",
  "text-sm font-semibold",
  "text-sm font-semibold text-text-secondary",
  "text-[13px] font-semibold text-text-secondary",
  "text-[13px] font-semibold text-text-secondary",
];

const ListView: React.FC<{ list: ListNode; path: string }> = ({
  list,
  path,
}) => {
  const items = list.items.map((item, i) => (
    <li key={`${path}-${i}`} className={item.task !== null ? "list-none" : ""}>
      {item.task !== null && (
        <span className="inline-flex align-[-2px] me-1.5 -ms-5 text-text-secondary">
          {item.task ? (
            <SquareCheck className="w-3.5 h-3.5" aria-hidden />
          ) : (
            <Square className="w-3.5 h-3.5" aria-hidden />
          )}
        </span>
      )}
      {renderInline(item.text, `${path}-${i}`)}
      {item.children.map((child, c) => (
        <ListView key={c} list={child} path={`${path}-${i}-${c}`} />
      ))}
    </li>
  ));
  return list.ordered ? (
    <ol start={list.start} className="list-decimal ps-5 space-y-0.5">
      {items}
    </ol>
  ) : (
    <ul className="list-disc ps-5 space-y-0.5">{items}</ul>
  );
};

const BlockView: React.FC<{ block: Block; path: string }> = ({
  block,
  path,
}) => {
  switch (block.kind) {
    case "heading": {
      const Tag = `h${Math.min(block.level + 1, 6)}` as "h2";
      return (
        <Tag className={`${HEADING_CLASSES[block.level - 1]} pt-1`}>
          {renderInline(block.text, path)}
        </Tag>
      );
    }
    case "paragraph":
      return <p>{renderLines(block.text, path)}</p>;
    case "code":
      return (
        <pre className="px-3 py-2 rounded-md bg-surface2 font-mono text-[12px] leading-[18px] overflow-x-auto whitespace-pre">
          {block.text}
        </pre>
      );
    case "rule":
      return <hr className="border-border" />;
    case "quote":
      return (
        <blockquote className="border-s-2 border-border ps-3 text-text-secondary space-y-2">
          {block.blocks.map((inner, i) => (
            <BlockView key={i} block={inner} path={`${path}-${i}`} />
          ))}
        </blockquote>
      );
    case "list":
      return <ListView list={block.list} path={path} />;
    case "table":
      return (
        <div className="overflow-x-auto">
          <table className="text-[13px] border-collapse">
            <thead>
              <tr>
                {block.header.map((cell, i) => (
                  <th
                    key={i}
                    className="border border-border px-2 py-1 text-start font-semibold bg-surface2"
                  >
                    {renderInline(cell, `${path}-h${i}`)}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {block.rows.map((row, r) => (
                <tr key={r}>
                  {block.header.map((_, c) => (
                    <td
                      key={c}
                      className="border border-border px-2 py-1 align-top"
                    >
                      {renderInline(row[c] ?? "", `${path}-${r}-${c}`)}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      );
  }
};

/** A note's Markdown as formatted text. */
export const Markdown: React.FC<{ markdown: string; className?: string }> = ({
  markdown,
  className = "",
}) => {
  const blocks = React.useMemo(() => parseMarkdown(markdown), [markdown]);
  return (
    <div className={`text-sm leading-relaxed space-y-2 ${className}`}>
      {blocks.map((block, i) => (
        <BlockView key={i} block={block} path={`b${i}`} />
      ))}
    </div>
  );
};
