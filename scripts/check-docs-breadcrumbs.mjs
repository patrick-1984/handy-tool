#!/usr/bin/env node
/**
 * check-docs-breadcrumbs.mjs — documentation UI-label drift checker for Handy Tool.
 *
 * WHY THIS EXISTS
 * ---------------
 * The docs contain NO screenshots (owner decision: the app moves too fast for images
 * to stay true). Navigation is expressed as TEXT BREADCRUMBS instead:
 *
 *     `General -> "Transcribe & Submit" -> "Paste method" = "Ctrl+V"`
 *
 * A breadcrumb is only better than a screenshot if it is MACHINE-CHECKED. This script
 * is that check: every segment in a canonical inline-code breadcrumb must be a
 * real user-visible English string from translation.json or nav-map.json. When a release
 * renames a setting, this fails and names the file, the line, and the likely new label.
 *
 * No dependencies. Runs on Bun or Node >= 18:
 *     bun run scripts/check-docs-breadcrumbs.mjs
 *     node    scripts/check-docs-breadcrumbs.mjs --fix-suggestions
 *
 * Exit codes: 0 = clean, 1 = unresolved labels, 2 = configuration error.
 */

import { readFileSync, readdirSync, existsSync } from "node:fs";
import { join, resolve, relative, sep as pathSep, dirname } from "node:path";
import { fileURLToPath } from "node:url";

/* ------------------------------------------------------------------ *
 * 0. Repo-root discovery + defaults
 * ------------------------------------------------------------------ */

const HERE = dirname(fileURLToPath(import.meta.url));

/** Walk up from a starting dir looking for the repo marker (package.json + src/i18n). */
function findRepoRoot(start) {
  let dir = resolve(start);
  for (let i = 0; i < 8; i++) {
    if (
      existsSync(join(dir, "package.json")) &&
      existsSync(join(dir, "src", "i18n", "locales", "en", "translation.json"))
    ) {
      return dir;
    }
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return null;
}

const DEFAULT_ROOT =
  findRepoRoot(HERE) || findRepoRoot(process.cwd()) || process.cwd();

/* ------------------------------------------------------------------ *
 * 1. CLI
 * ------------------------------------------------------------------ */

const HELP = `
check-docs-breadcrumbs — fail when the docs reference a UI label the app no longer renders.

USAGE
  node check-docs-breadcrumbs.mjs [options]

OPTIONS
  --docs <dir>          Docs root to scan recursively for *.md   (default: <repo>/docs)
  --i18n <file>         Truth source of user-visible strings
                        (default: <repo>/src/i18n/locales/en/translation.json)
  --nav-map <file>      Optional nav-map.json (see contract in checker-README.md).
                        Absent => falls back to translation.json alone.
  --sep <list>          Comma-separated breadcrumb separators
                        (default: "->,\u2192,>,\u203a,\u00bb")
  --threshold <0..1>    Minimum similarity to offer a "did you mean" (default: 0.62)
  --fix-suggestions     Print a patch-style list of proposed replacements. Writes NOTHING.
  --strict              Also FAIL on unquoted breadcrumb segments (default: advisory only).
  --warn-only           Always exit 0. For the non-blocking first phase in CI.
  --json                Emit a machine-readable JSON report on stdout.
  --loose               Recognise breadcrumbs in bare prose (no anchor required). Debug aid.
  --quiet               Only print the summary line.
  -h, --help            This text.

OPT-OUT MARKERS (documented for doc authors)
  <!-- drift-ok -->                 skip this line (and the line after, if the marker is
                                    on its own line) - for intentionally generic references
  <!-- drift-ok: "A", "B" -->       skip only these labels on this line / the next line
  <!-- drift-ok-file -->            skip the entire file (put it near the top)
  Fenced code blocks (\`\`\`) are always skipped.
`;

function parseArgs(argv) {
  const o = {
    docs: null,
    i18n: null,
    navMap: null,
    sep: null,
    threshold: 0.62,
    fixSuggestions: false,
    strict: false,
    warnOnly: false,
    json: false,
    loose: false,
    quiet: false,
    help: false,
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const val = () => {
      const inline = a.indexOf("=");
      if (inline > -1) return a.slice(inline + 1);
      return argv[++i];
    };
    const name = a.split("=")[0];
    switch (name) {
      case "--docs": o.docs = val(); break;
      case "--i18n": o.i18n = val(); break;
      case "--nav-map": o.navMap = val(); break;
      case "--sep": o.sep = val(); break;
      case "--threshold": o.threshold = Number(val()); break;
      case "--fix-suggestions": o.fixSuggestions = true; break;
      case "--strict": o.strict = true; break;
      case "--warn-only": o.warnOnly = true; break;
      case "--json": o.json = true; break;
      case "--loose": o.loose = true; break;
      case "--quiet": o.quiet = true; break;
      case "-h": case "--help": o.help = true; break;
      default:
        if (name.startsWith("-")) {
          console.error(`Unknown option: ${name}`);
          process.exit(2);
        }
    }
  }
  return o;
}

const opts = parseArgs(process.argv.slice(2));
if (opts.help) { console.log(HELP.trim()); process.exit(0); }

const DOCS_DIR = resolve(opts.docs || join(DEFAULT_ROOT, "docs"));
const I18N_FILE = resolve(
  opts.i18n || join(DEFAULT_ROOT, "src", "i18n", "locales", "en", "translation.json")
);
const NAV_MAP_FILE = resolve(
  opts.navMap ||
    join(HERE, "nav-map.json") // sibling agent writes it next to this script by default
);
const SEPARATORS = (opts.sep ? opts.sep.split(",") : ["->", "\u2192", ">", "\u203a", "\u00bb"])
  .map((s) => s.trim())
  .filter(Boolean);

/* ------------------------------------------------------------------ *
 * 2. Truth source: flatten translation.json into a Set of every string value
 * ------------------------------------------------------------------ */

/** Flatten nested i18n JSON => [{ key, value }] for every string leaf (incl. arrays). */
function flattenTranslations(node, prefix = "", out = []) {
  if (typeof node === "string") {
    out.push({ key: prefix, value: node });
    return out;
  }
  if (Array.isArray(node)) {
    node.forEach((v, i) => flattenTranslations(v, `${prefix}[${i}]`, out));
    return out;
  }
  if (node && typeof node === "object") {
    for (const k of Object.keys(node)) {
      flattenTranslations(node[k], prefix ? `${prefix}.${k}` : k, out);
    }
  }
  return out;
}

/**
 * Normalisation used for the "same label, cosmetically different" comparison.
 * Deliberately aggressive: case, punctuation, dash/space variants, trailing
 * colons/ellipses and i18n interpolation placeholders all collapse away.
 */
function normalize(s) {
  return String(s)
    .replace(/\{\{[^}]*\}\}/g, " ")          // i18n interpolation {{count}}
    .replace(/[\u2018\u2019]/g, "'")          // curly single quotes
    .replace(/[\u201c\u201d]/g, '"')          // curly double quotes
    .replace(/[\u2010-\u2015\u2212]/g, "-")   // all dash flavours -> hyphen
    .replace(/[\u2026]/g, "...")
    .toLowerCase()
    .replace(/&/g, " and ")
    .replace(/[^a-z0-9]+/g, " ")
    .trim();
}

if (!existsSync(I18N_FILE)) {
  console.error(`FATAL: translation source not found: ${I18N_FILE}`);
  console.error(`       Pass --i18n <file> if the repo layout changed.`);
  process.exit(2);
}

let translationPairs;
try {
  translationPairs = flattenTranslations(JSON.parse(readFileSync(I18N_FILE, "utf8")));
} catch (e) {
  console.error(`FATAL: could not parse ${I18N_FILE}: ${e.message}`);
  process.exit(2);
}

/** Every exact user-visible string the app can render. */
const LABELS = new Set(translationPairs.map((p) => p.value));
const INTERPOLATED_LABELS = translationPairs
  .map((p) => p.value)
  .filter((value) => /\{\{[^}]+\}\}/.test(value));

/** Labels supplied by nav-map.json are also authoritative UI strings. */
const NAV_LABELS = new Set();
const NAV_INTERPOLATED_LABELS = [];

/** Intentional non-settings breadcrumb roots (closed vocabulary). */
const RESERVED_LABELS = new Set(["Tray", "Shortcut", "CLI", "File"]);

function matchesInterpolatedLabel(label, templates) {
  return templates.some((template) => {
    // A placeholder-only value such as "{{file}}" is runtime content, not a UI
    // label pattern. Treating it as ^.+?$ would make the checker vacuous.
    const literalText = template.replace(/\{\{[^}]+\}\}/g, "");
    if (!/[\p{L}\p{N}]/u.test(literalText)) return false;

    const source = template
      .split(/(\{\{[^}]+\}\})/g)
      .map((part) =>
        /^\{\{[^}]+\}\}$/.test(part)
          ? /^(?:index|count|percent|percentage|seconds|ms|start|end|total|done|failed|matched|typed)$/i
              .test(part.slice(2, -2).trim())
            ? "\\d+(?:\\.\\d+)?"
            : ".+?"
          : part.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"),
      )
      .join("");
    return new RegExp(`^${source}$`).test(label);
  });
}

/** Accept an exact translation or a concrete UI rendering of an i18n template. */
function isKnownLabel(label) {
  return LABELS.has(label) || NAV_LABELS.has(label) || RESERVED_LABELS.has(label) ||
    matchesInterpolatedLabel(label, INTERPOLATED_LABELS) ||
    matchesInterpolatedLabel(label, NAV_INTERPOLATED_LABELS);
}

/** Used only to diagnose keyed nav-map entries that drifted from i18n. */
function isKnownTranslationLabel(label) {
  return LABELS.has(label) || matchesInterpolatedLabel(label, INTERPOLATED_LABELS);
}
/** normalized -> [{key,value}] for near-miss / cosmetic-drift detection. */
const BY_NORM = new Map();
for (const p of translationPairs) {
  const n = normalize(p.value);
  if (!n) continue;
  if (!BY_NORM.has(n)) BY_NORM.set(n, []);
  BY_NORM.get(n).push(p);
}
/** value -> first i18n key, for reporting provenance. */
const KEY_OF = new Map();
for (const p of translationPairs) if (!KEY_OF.has(p.value)) KEY_OF.set(p.value, p.key);

/* ------------------------------------------------------------------ *
 * 3. Optional nav-map.json (contract documented in checker-README.md)
 * ------------------------------------------------------------------ */

/**
 * EXPECTED CONTRACT (all fields optional except `label`):
 *   { "entries": [ { "page":"Advanced", "group":"Transcription",
 *                    "control":"dropdown", "label":"Transcription Mode",
 *                    "key":"settings.advanced.transcriptionMode.title",
 *                    "options":[{"label":"Live","key":"...options.live"}] } ] }
 * A bare top-level array is also accepted.
 *
 * The nav-map is an ENRICHMENT, never a second truth source: translation.json
 * decides pass/fail. The nav-map buys us (a) page-scoped "did you mean"
 * suggestions and (b) detection of nav-map entries that have themselves drifted.
 */
const navMap = { loaded: false, entries: [], labelsByPage: new Map(), stale: [] };

function loadNavMap(file) {
  if (!existsSync(file)) return;
  let raw;
  try {
    raw = JSON.parse(readFileSync(file, "utf8"));
  } catch (e) {
    navMap.parseError = e.message;
    return;
  }
  const entries = Array.isArray(raw) ? raw : Array.isArray(raw?.entries) ? raw.entries : [];
  if (!entries.length) return;
  navMap.loaded = true;
  for (const e of entries) {
    if (!e || typeof e !== "object") continue;
    // Field-name tolerance. Two shapes are accepted:
    //   generic : { label, key,      control: <type> }
    //   generator: { control: <label>, titleKey, type: <type> }   <- nav-map.json today
    // `control` is the LABEL when there is no explicit `label` field, and the control
    // TYPE when there is. Everything else falls back through the aliases.
    const label =
      typeof e.label === "string" ? e.label
      : typeof e.control === "string" ? e.control
      : typeof e.title === "string" ? e.title
      : null;
    if (!label) continue;
    const rec = {
      page: e.page ?? null,
      tab: e.tab ?? null,
      group: e.group ?? null,
      sidebarGroup: e.sidebarGroup ?? null,
      control: e.type ?? (typeof e.label === "string" ? e.control : null) ?? null,
      label,
      key: e.key ?? e.titleKey ?? null,
      options: Array.isArray(e.options)
        ? e.options.map((o) => (typeof o === "string" ? o : o?.label)).filter(Boolean)
        : [],
      labels: Array.isArray(e.labels) ? e.labels.filter((v) => typeof v === "string") : [],
    };
    navMap.entries.push(rec);
    const authoritativeLabels = [
      rec.page, rec.tab, rec.group, rec.sidebarGroup, rec.label, ...rec.options, ...rec.labels,
    ]
      .filter((value) => typeof value === "string" && value.length > 0);
    for (const value of authoritativeLabels) {
      NAV_LABELS.add(value);
      if (/\{\{[^}]+\}\}/.test(value) && !NAV_INTERPOLATED_LABELS.includes(value)) {
        NAV_INTERPOLATED_LABELS.push(value);
      }
    }
    for (const scope of [rec.page, rec.tab, rec.group, rec.sidebarGroup].filter(Boolean)) {
      if (!navMap.labelsByPage.has(scope)) navMap.labelsByPage.set(scope, []);
      navMap.labelsByPage.get(scope).push(rec);
    }
    // A nav-map CONTROL LABEL that is not in translation.json means the nav-map is
    // stale (or the label is composed at runtime from an interpolated string).
    //
    // Options are deliberately NOT checked here: dropdown values are very often
    // untranslated literals — tool names ("xdotool"), units ("100 ms"), model ids —
    // and reporting them buries the real signal. They are still loaded, because the
    // "valid options are ..." suggestion depends on them.
    const isPlaceholder =
      /^<.*>$/.test(rec.label) ||        // doc placeholder: <model name>
      rec.label.includes("…") ||     // collapsed range: "Slot 2 ... Slot 9"
      rec.label.includes(" / ");          // collapsed alternatives
    // A null key denotes a runtime/hardcoded control label; translation.json cannot
    // validate it. Keyed labels, including concrete renderings of templates, must match.
    if (rec.key && !isKnownTranslationLabel(rec.label) && !isPlaceholder) {
      navMap.stale.push({ label: rec.label, page: rec.page, key: rec.key });
    }
  }
}
loadNavMap(NAV_MAP_FILE);
const KNOWN_LABEL_COUNT = new Set([...LABELS, ...NAV_LABELS, ...RESERVED_LABELS]).size;

/* ------------------------------------------------------------------ *
 * 4. String similarity (built-in, no dependencies)
 * ------------------------------------------------------------------ */

/** Classic Levenshtein distance, two-row rolling buffer. */
function levenshtein(a, b) {
  if (a === b) return 0;
  if (!a.length) return b.length;
  if (!b.length) return a.length;
  let prev = new Array(b.length + 1);
  let cur = new Array(b.length + 1);
  for (let j = 0; j <= b.length; j++) prev[j] = j;
  for (let i = 1; i <= a.length; i++) {
    cur[0] = i;
    const ca = a.charCodeAt(i - 1);
    for (let j = 1; j <= b.length; j++) {
      const cost = ca === b.charCodeAt(j - 1) ? 0 : 1;
      cur[j] = Math.min(cur[j - 1] + 1, prev[j] + 1, prev[j - 1] + cost);
    }
    const t = prev; prev = cur; cur = t;
  }
  return prev[b.length];
}

/** 0..1 similarity from edit distance. */
function ratio(a, b) {
  const m = Math.max(a.length, b.length);
  return m === 0 ? 1 : 1 - levenshtein(a, b) / m;
}

/** Jaccard overlap of word sets — catches reorderings Levenshtein punishes. */
function tokenOverlap(a, b) {
  const A = new Set(a.split(" ").filter(Boolean));
  const B = new Set(b.split(" ").filter(Boolean));
  if (!A.size || !B.size) return 0;
  let inter = 0;
  for (const t of A) if (B.has(t)) inter++;
  return inter / (A.size + B.size - inter);
}

/**
 * Rank candidate replacements for an unresolved label.
 * `scopeHints` are the breadcrumb's ancestor segments (page/group names); a
 * candidate that lives under one of them in the nav-map gets a confidence bump.
 */
function suggest(label, scopeHints = [], limit = 3, parentControl = null) {
  const target = normalize(label);
  if (!target) return [];

  // Strongest signal available: the doc says this is an OPTION of a control, and the
  // nav-map knows that control's real option list. Enumerate the valid options
  // outright — string similarity cannot know that "Realtime" should be "Live".
  if (parentControl) {
    const rec = navMap.entries.find((e) => e.label === parentControl && e.options.length);
    if (rec) {
      return rec.options.slice(0, 6).map((v) => ({
        value: v, key: KEY_OF.get(v) || rec.key, score: 1, scoped: true, validOption: true,
      }));
    }
  }

  const scoped = new Set();
  for (const hint of scopeHints) {
    for (const rec of navMap.labelsByPage.get(hint) || []) {
      scoped.add(rec.label);
      for (const o of rec.options) scoped.add(o);
    }
  }

  const scored = [];
  for (const p of translationPairs) {
    const cand = normalize(p.value);
    if (!cand) continue;
    // Long prose strings are never breadcrumb leaves; skip to keep suggestions sane.
    if (p.value.length > 80) continue;
    let score = Math.max(ratio(target, cand), tokenOverlap(target, cand) * 0.95);
    if (cand === target) score = 1;                       // cosmetic-only difference
    else if (cand.includes(target) || target.includes(cand)) score = Math.max(score, 0.8);
    if (scoped.has(p.value)) score = Math.min(1, score + 0.08);
    if (score > 0) scored.push({ value: p.value, key: p.key, score, scoped: scoped.has(p.value) });
  }
  for (const value of NAV_LABELS) {
    if (LABELS.has(value)) continue;
    const cand = normalize(value);
    if (!cand || value.length > 80) continue;
    let score = Math.max(ratio(target, cand), tokenOverlap(target, cand) * 0.95);
    if (cand === target) score = 1;
    else if (cand.includes(target) || target.includes(cand)) score = Math.max(score, 0.8);
    if (scoped.has(value)) score = Math.min(1, score + 0.08);
    if (score > 0) scored.push({
      value,
      key: KEY_OF.get(value) || null,
      score,
      scoped: scoped.has(value),
    });
  }

  scored.sort((a, b) => b.score - a.score || a.value.length - b.value.length);
  const seen = new Set();
  const out = [];
  for (const s of scored) {
    if (s.score < opts.threshold) break;
    if (seen.has(s.value)) continue;
    seen.add(s.value);
    out.push(s);
    if (out.length >= limit) break;
  }
  return out;
}

/* ------------------------------------------------------------------ *
 * 5. Docs discovery
 * ------------------------------------------------------------------ */

const SKIP_DIRS = new Set(["node_modules", ".git", "target", "dist", "build", ".next"]);

/** Repo-relative POSIX path for reporting; falls back to absolute when outside the repo. */
function displayPath(abs) {
  const rel = relative(DEFAULT_ROOT, abs);
  if (!rel || rel.startsWith("..")) return abs.split(pathSep).join("/");
  return rel.split(pathSep).join("/");
}

function walkMarkdown(dir, acc = []) {
  let entries;
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    return acc;
  }
  for (const e of entries) {
    const full = join(dir, e.name);
    if (e.isDirectory()) {
      if (SKIP_DIRS.has(e.name)) continue;
      walkMarkdown(full, acc);
    } else if (e.isFile() && /\.mdx?$/i.test(e.name)) {
      acc.push(full);
    }
  }
  return acc;
}

/* ------------------------------------------------------------------ *
 * 6. Breadcrumb parsing
 * ------------------------------------------------------------------ */

const escapeRe = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/**
 * A separator only counts when it is surrounded by whitespace (or is a unicode
 * arrow). This is what keeps `<div>`, markdown blockquotes and HTML out of the
 * parse, and is why prose like "(small -> large-v3)" needs a quote to qualify.
 */
const UNICODE_SEPARATORS = new Set(["\u2192", "\u203a", "\u00bb"]);
const SEP_RE_SRC = SEPARATORS.map((s) =>
  UNICODE_SEPARATORS.has(s)
    ? `\\s*${escapeRe(s)}\\s*`
    : `\\s+${escapeRe(s)}\\s+`,
).join("|");
const SEP_RE = new RegExp(`(?:${SEP_RE_SRC})`);
const SEP_RE_G = new RegExp(`(?:${SEP_RE_SRC})`, "g");

// One breadcrumb segment: a quoted run, or a bare run of label-ish characters.
const QUOTED_SEG = `"[^"\\n]+"|'[^'\\n]+'|\u201c[^\u201d\\n]+\u201d|\`[^\`\\n]+\``;
// Tempered greedy token: consume label characters greedily, but never step across a
// separator. (A lazy `*?` here would truncate "Models" to "M"; a plain greedy `*`
// would swallow the "-" of a "->" separator, since "-" is legal inside a label.)
const BARE_CHAR = `[A-Za-z0-9 &/'\u2019+.,:\u2026\\-*_()]`;
const BARE_SEG = `[A-Za-z0-9](?:(?!${SEP_RE_SRC})${BARE_CHAR})*`;
const SEG = `(?:${QUOTED_SEG}|${BARE_SEG})`;
const CHAIN_RE = new RegExp(`${SEG}(?:(?:${SEP_RE_SRC})${SEG})+`, "g");

// Trailing dropdown selection: ... = "Live"  /  ... set to "Live"  /  ... : "Live"
const OPTION_RE = new RegExp(`^\\s*(?:=|:|set to|choose|select)\\s*(${QUOTED_SEG})`, "i");

// Anchors that mark a chain as "this is navigation, not prose".
const INLINE_CODE_RE = /`([^`\n]+)`/g;
const BOLD_RE = /(?:\*\*|__)([^*_\n][^\n]*?)(?:\*\*|__)/g;
const NAV_PREFIX_RE = /(?:^|[\s(])(?:Nav|Path|Go to|Navigate|Location|Where)\s*:\s*(.+)$/i;

const DRIFT_OK_LINE_RE = /<!--\s*drift-ok\s*-->/i;
const DRIFT_OK_SEL_RE = /<!--\s*drift-ok\s*:\s*([^>]*?)\s*-->/i;
const DRIFT_OK_FILE_RE = /<!--\s*drift-ok-file\s*-->/i;

const SHORTCUT_VALUE_RE =
  /^(?:ctrl|alt|shift|super|cmd|win)(?:\+(?:ctrl|alt|shift|super|cmd|win|space|enter|tab|escape|[a-z0-9]))+$/i;
const CLI_TARGET_RE = /^handy(?:\s+(?:--?[a-z0-9][a-z0-9-]*|[a-z0-9][a-z0-9-]*))*$/i;
const FILE_TARGET_RE = /^(?:%APPDATA%\\pr\.handy(?:\\[A-Za-z0-9._-]+)*|portable\.marker)$/;

function isKnownContextualSegment(segment, root, parentControl) {
  if (root === "CLI") return CLI_TARGET_RE.test(segment.text);
  if (root === "File") return FILE_TARGET_RE.test(segment.text);
  if (root === "Shortcut") return SHORTCUT_VALUE_RE.test(segment.text);
  if (!segment.isOption || !parentControl || !SHORTCUT_VALUE_RE.test(segment.text)) return false;
  return navMap.entries.some(
    (entry) => entry.label === parentControl && entry.control === "shortcut recorder",
  );
}

const isQuoted = (s) =>
  /^".*"$/.test(s) || /^'.*'$/.test(s) || /^\u201c.*\u201d$/.test(s) || /^`.*`$/.test(s);

/** Strip surrounding quotes and markdown emphasis from a raw segment. */
function cleanSegment(raw) {
  let s = raw.trim();
  const quoted = isQuoted(s);
  if (quoted) s = s.slice(1, -1);
  s = s.replace(/^(\*\*|__|\*|_|`)+/, "").replace(/(\*\*|__|\*|_|`)+$/, "");
  return { text: s.trim(), quoted };
}

/** Parse one chain of text into segments. */
function splitChain(text) {
  return text
    .split(SEP_RE_G)
    .map((raw) => cleanSegment(raw))
    .filter((s) => s.text.length > 0);
}

/**
 * Extract every breadcrumb on a line.
 * Returns [{ segments, raw, anchor }].
 *
 * Anchoring rules (this is the false-positive guard):
 *   - inline code span containing a separator          -> always a breadcrumb
 *   - bold span containing a separator                 -> always a breadcrumb
 *   - a "Nav:"/"Path:"/"Go to:" prefix                 -> always a breadcrumb
 *   - a free-prose chain that contains >=1 QUOTED seg  -> a breadcrumb
 *   - anything else                                    -> ignored (prose arrows,
 *     value ranges like "None -> 5 s", "small -> large-v3")
 * `--loose` drops the last requirement.
 */
function extractBreadcrumbs(line) {
  const found = [];
  const seen = new Set();

  const push = (text, anchor) => {
    if (!text || !SEP_RE.test(text)) return;

    // Canonical docs notation is one whole inline-code span. Parse that span
    // directly instead of feeding it through the legacy quoted/bare token regex:
    // doing so preserves commas, parentheses, and an unquoted trailing option.
    if (anchor === "code") {
      const segments = splitChain(text);
      if (segments.length < 2) return;

      const last = segments.pop();
      const optionMatch = /^(.+?)\s+=\s+(.+)$/.exec(last.text);
      if (optionMatch) {
        const control = cleanSegment(optionMatch[1]);
        const option = cleanSegment(optionMatch[2]);
        if (control.text) segments.push(control);
        if (option.text) segments.push({ ...option, isOption: true });
      } else {
        segments.push(last);
      }

      const dedupe = segments.map((s) => s.text).join("\u0000");
      if (seen.has(dedupe)) return;
      seen.add(dedupe);
      found.push({ segments, raw: text, anchor });
      return;
    }

    CHAIN_RE.lastIndex = 0;
    let m;
    while ((m = CHAIN_RE.exec(text)) !== null) {
      const chainText = m[0];
      const segments = splitChain(chainText);
      if (segments.length < 2) continue;

      // A trailing "= \"Option\"" immediately after the chain is a dropdown value.
      const after = text.slice(m.index + chainText.length);
      const om = OPTION_RE.exec(after);
      if (om) {
        const c = cleanSegment(om[1]);
        if (c.text) segments.push({ ...c, isOption: true });
      }

      const hasQuoted = segments.some((s) => s.quoted);
      if (anchor === "prose" && !hasQuoted && !opts.loose) continue;

      // Dedupe on CONTENT ONLY, never on the anchor: the same chain is normally
      // found twice (once by its code/bold anchor, once by the prose sweep of the
      // whole line). Anchors are tried most-specific-first, so the first match wins
      // and keeps the better provenance. Two genuinely different chains on one line
      // still differ in content, so both are still reported.
      const dedupe = segments.map((s) => s.text).join("\u0000");
      if (seen.has(dedupe)) continue;
      seen.add(dedupe);
      found.push({ segments, raw: chainText + (om ? om[0] : ""), anchor });
    }
  };

  let m;
  INLINE_CODE_RE.lastIndex = 0;
  while ((m = INLINE_CODE_RE.exec(line)) !== null) push(m[1], "code");
  BOLD_RE.lastIndex = 0;
  while ((m = BOLD_RE.exec(line)) !== null) push(m[1], "bold");
  const nav = NAV_PREFIX_RE.exec(line);
  if (nav) push(nav[1], "nav-prefix");
  push(line, "prose");

  return found;
}

/* ------------------------------------------------------------------ *
 * 7. Scan
 * ------------------------------------------------------------------ */

if (!existsSync(DOCS_DIR)) {
  console.error(`FATAL: docs directory not found: ${DOCS_DIR}`);
  process.exit(2);
}

const files = walkMarkdown(DOCS_DIR).sort();

const problems = [];   // unresolved canonical labels -> FAIL
const advisories = []; // unquoted segments, unknown -> advisory (FAIL only with --strict)
const stats = {
  files: files.length,
  breadcrumbs: 0,
  checked: 0,
  ok: 0,
  skipped: 0,
  filesSkipped: 0,
};

for (const file of files) {
  const text = readFileSync(file, "utf8");
  if (DRIFT_OK_FILE_RE.test(text)) { stats.filesSkipped++; continue; }

  const lines = text.split(/\r?\n/);
  const rel = displayPath(file);

  let inFence = false;
  let carryAllowAll = false;   // <!-- drift-ok --> on its own line applies to the next line
  let carryAllowList = null;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];

    if (/^\s*(```|~~~)/.test(line)) { inFence = !inFence; continue; }
    if (inFence) continue;

    // --- opt-out handling -------------------------------------------------
    let allowAll = carryAllowAll;
    let allowList = carryAllowList;
    carryAllowAll = false;
    carryAllowList = null;

    const sel = DRIFT_OK_SEL_RE.exec(line);
    const bare = DRIFT_OK_LINE_RE.test(line);
    const markerOnly = /^\s*<!--\s*drift-ok[^>]*-->\s*$/i.test(line);

    if (sel) {
      const list = (sel[1].match(/"[^"]+"|'[^']+'/g) || []).map((s) => s.slice(1, -1));
      if (markerOnly) carryAllowList = list; else allowList = (allowList || []).concat(list);
    } else if (bare) {
      if (markerOnly) carryAllowAll = true; else allowAll = true;
    }
    if (markerOnly) continue;
    if (allowAll) { stats.skipped++; continue; }

    // --- breadcrumbs ------------------------------------------------------
    for (const crumb of extractBreadcrumbs(line)) {
      stats.breadcrumbs++;
      const segTexts = crumb.segments.map((s) => s.text);
      const root = crumb.segments[0]?.text;

      for (let s = 0; s < crumb.segments.length; s++) {
        const segment = crumb.segments[s];
        const isLast = s === crumb.segments.length - 1;
        const role = segment.isOption ? "option" : isLast ? "leaf" : "crumb";
        // For a dropdown value, the control it belongs to is the preceding segment.
        const parentControl = segment.isOption && s > 0 ? crumb.segments[s - 1].text : null;

        if (allowList && allowList.includes(segment.text)) { stats.skipped++; continue; }

        // Canonical inline-code breadcrumbs are strict regardless of quoting.
        // Preserve the old advisory behavior only for legacy bold/prose paths.
        if (!segment.quoted && crumb.anchor !== "code") {
          if (!isKnownLabel(segment.text)) {
            advisories.push({
              file: rel, line: i + 1, label: segment.text, role,
              breadcrumb: crumb.raw, anchor: crumb.anchor,
              suggestions: suggest(segment.text, segTexts.slice(0, s)),
            });
          }
          continue;
        }

        stats.checked++;
        const reservedInWrongPosition = RESERVED_LABELS.has(segment.text) && s !== 0;
        if (
          (!reservedInWrongPosition && isKnownLabel(segment.text)) ||
          (s > 0 && isKnownContextualSegment(segment, root, parentControl))
        ) {
          stats.ok++;
          continue;
        }

        problems.push({
          file: rel, line: i + 1, label: segment.text, role,
          breadcrumb: crumb.raw, anchor: crumb.anchor,
          column: Math.max(0, line.indexOf(segment.text)) + 1,
          suggestions: suggest(segment.text, segTexts.slice(0, s), 3, parentControl),
        });
      }
    }
  }
}

/* ------------------------------------------------------------------ *
 * 8. Report
 * ------------------------------------------------------------------ */

const C = process.stdout.isTTY && !process.env.NO_COLOR
  ? { red: "\x1b[31m", yellow: "\x1b[33m", green: "\x1b[32m", dim: "\x1b[2m", bold: "\x1b[1m", off: "\x1b[0m" }
  : { red: "", yellow: "", green: "", dim: "", bold: "", off: "" };

const fmtSuggestions = (list) => {
  if (!list.length) return null;
  if (list[0].validOption) {
    return `valid options are ${list.map((s) => `"${s.value}"`).join(", ")}`;
  }
  return list
    .map((s) => `"${s.value}" (${Math.round(s.score * 100)}%${s.scoped ? ", same page" : ""})`)
    .join("  |  ");
};

if (opts.json) {
  console.log(JSON.stringify({
    ok: problems.length === 0 && (!opts.strict || advisories.length === 0),
    stats,
    truthSource: displayPath(I18N_FILE),
    labelCount: KNOWN_LABEL_COUNT,
    navMap: { loaded: navMap.loaded, entries: navMap.entries.length, stale: navMap.stale },
    problems, advisories,
  }, null, 2));
} else if (opts.fixSuggestions) {
  console.log(`${C.bold}Proposed replacements (patch preview — nothing is written)${C.off}\n`);
  const all = problems.concat(opts.strict ? advisories : []);
  if (!all.length) {
    console.log("  (no unresolved labels — nothing to suggest)");
  }
  let current = null;
  for (const p of all) {
    if (p.file !== current) { current = p.file; console.log(`${C.bold}--- ${p.file}${C.off}`); }
    const best = p.suggestions[0];
    if (best) {
      console.log(`@@ line ${p.line} @@ (${p.role})`);
      console.log(`${C.red}-  "${p.label}"${C.off}`);
      console.log(`${C.green}+  "${best.value}"${C.off}   ${C.dim}# ${best.key} — ${Math.round(best.score * 100)}% match${C.off}`);
      if (p.suggestions.length > 1) {
        console.log(`${C.dim}   alternatives: ${p.suggestions.slice(1).map((s) => `"${s.value}"`).join(", ")}${C.off}`);
      }
    } else {
      console.log(`@@ line ${p.line} @@ (${p.role})`);
      console.log(`${C.red}-  "${p.label}"${C.off}`);
      console.log(`${C.dim}?  no candidate above ${Math.round(opts.threshold * 100)}% — the control may have been REMOVED.`);
      console.log(`${C.dim}   Rewrite the breadcrumb, or mark it <!-- drift-ok --> if intentionally generic.${C.off}`);
    }
    console.log("");
  }
} else if (!opts.quiet) {
  if (problems.length) {
    console.log(`${C.red}${C.bold}UNRESOLVED UI LABELS${C.off} ${C.dim}(breadcrumb segments absent from the UI label truth sources)${C.off}\n`);
    for (const p of problems) {
      console.log(`  ${C.bold}${p.file}:${p.line}:${p.column}${C.off}  ${C.red}"${p.label}"${C.off} ${C.dim}(${p.role})${C.off}`);
      console.log(`    ${C.dim}in: ${p.breadcrumb}${C.off}`);
      const s = fmtSuggestions(p.suggestions);
      console.log(s ? `    did you mean: ${C.green}${s}${C.off}` : `    ${C.yellow}no close match — control may have been removed${C.off}`);
      console.log("");
    }
  }
  if (advisories.length) {
    console.log(`${C.yellow}${C.bold}ADVISORY${C.off} ${C.dim}(unquoted breadcrumb segments — not verifiable against i18n; quote them to enforce)${C.off}`);
    for (const a of advisories) {
      const s = fmtSuggestions(a.suggestions);
      console.log(`  ${a.file}:${a.line}  ${C.yellow}${a.label}${C.off} ${C.dim}in "${a.breadcrumb}"${C.off}${s ? `  ${C.dim}~ ${s}${C.off}` : ""}`);
    }
    console.log("");
  }
  if (navMap.stale.length) {
    console.log(`${C.yellow}NAV-MAP DRIFT${C.off} ${C.dim}(nav-map.json labels absent from translation.json)${C.off}`);
    for (const s of navMap.stale.slice(0, 20)) console.log(`  ${C.yellow}"${s.label}"${C.off} ${C.dim}${s.page || ""} ${s.key || ""}${C.off}`);
    console.log("");
  }
  if (navMap.parseError) {
    console.log(`${C.yellow}WARN${C.off} nav-map.json present but unparseable (${navMap.parseError}); continuing with translation.json alone.\n`);
  }
}

const navNote = navMap.loaded
  ? `nav-map: ${navMap.entries.length} entries`
  : `nav-map: absent (translation.json only)`;

if (!opts.json) {
  const verdict = problems.length
    ? `${C.red}FAIL${C.off}`
    : advisories.length && opts.strict
      ? `${C.red}FAIL${C.off}`
      : `${C.green}PASS${C.off}`;
  console.log(
    `${verdict}  ${stats.files} file(s), ${stats.breadcrumbs} breadcrumb(s), ` +
    `${stats.checked} label(s) checked, ${stats.ok} resolved, ` +
    `${problems.length} unresolved, ${advisories.length} advisory, ` +
    `${stats.skipped} opted out${stats.filesSkipped ? `, ${stats.filesSkipped} file(s) skipped` : ""}.  ` +
    `${C.dim}${KNOWN_LABEL_COUNT} known labels; ${navNote}${C.off}`
  );
}

const failed = problems.length > 0 || (opts.strict && advisories.length > 0);
process.exit(opts.warnOnly ? 0 : failed ? 1 : 0);
