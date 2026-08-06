# Documentation contract

This file is the mechanical rulebook for Handy Tool documentation. Follow it literally. When a rule conflicts with a convenient rewrite, the rule wins.

Two facts constrain every page:

- The current version is 1.0.0. Never cite an earlier version as current.
- Windows x64 is the only build produced, tested, and released. macOS and Linux are planned and in the queue. Never write that a macOS or Linux build exists, is downloadable, or is testable, and mark a Windows-only capability with _{Windows only}_.

Prose is US English (behavior, memorized, capitalization, recognized, center), because the interface strings the pages quote are US English.

## One prose home

`docs/features.md` is the canonical catalog. A sentence that explains what Handy Tool does, when a capability applies, or why its behavior matters belongs in that file and nowhere else.

Outside the catalog, refer to a capability with one Markdown link. Its visible text must equal the catalog heading character-for-character, and its target must use the entry’s explicit slug:

```markdown
[The paste didn't land — get the words back without re-dictating](features.md#the-paste-didnt-land-get-the-words-back)
```

Adjust only the relative path:

| Source page                                                    | Target form              |
| -------------------------------------------------------------- | ------------------------ |
| Repository `README.md`                                         | `docs/features.md#slug`  |
| `docs/*.md`                                                    | `features.md#slug`       |
| `docs/tools/*.md`, `docs/start/*.md`, or `docs/reference/*.md` | `../features.md#slug`    |
| `docs/reference/settings/*.md`                                 | `../../features.md#slug` |

Do not copy, shorten, summarize, or paraphrase catalog descriptions. Setting labels, breadcrumbs, key chords, platform badges, and linked feature names may repeat.

The bounded exceptions are:

- The root README may carry the four-beat owner story, one product-definition sentence, installation and platform facts, the left-hand deck teaser, and a capability list made only of catalog links. Its defaults block points to the catalog rather than reproducing the defaults.
- A tool page may carry its assigned owner story and setup walkthrough. Neither may become a second feature description.
- A learning-path page may give the instructions needed to complete its exercise. Any claim about a capability links to the catalog.
- `docs/improvements.md` may explain how a guarantee was engineered. It links the guarantee’s catalog entry and does not redefine what the feature does.

## Page contracts

| Page type                      | Contract                                                                                                                                                                                                                                                               |
| ------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `docs/README.md`               | Navigation only: the path, the tools, and the reference shelf. Each item is a link plus one clause describing what the destination page contains. No feature statements. Link text for a page equals that page's `#` heading; link text for a catalog entry equals its catalog heading.                                                     |
| `docs/features.md`             | The only full capability descriptions. Entries use explicit frozen anchors and the catalog template below.                                                                                                                                                             |
| `docs/tools/*.md`              | Owner story and walkthrough prose, with the three link-only blocks defined below.                                                                                                                                                                                      |
| `docs/start/*.md`              | One numbered learning path. Each rung carries a `**Platform:**` line, introduces at most five settings, links the next rung with text equal to that rung's `#` heading, and ends with a bold `**You can stop here.**` followed by one clause naming what the reader gained. Hardware first appears at rung 07, and rungs before it require nothing but the machine the reader has.                                     |
| `docs/reference/settings/*.md` | One page per application screen. Controls stay in screen order and use exact live labels, exact breadcrumbs, explicit defaults, and catalog links where available.                                                                                                     |
| `docs/reference/*.md`          | Lookup tables and definitions. See the reference allowances below.                                                                                                                                                                                                     |
| `docs/privacy.md`              | Evidence-based data-flow boundaries. Do not strengthen a claim beyond the audited source.                                                                                                                                                                              |

## Tool-page link-only blocks

Every tool page uses this shape in order:

1. The assigned owner story in second person, as an unlabeled lead paragraph under the title.
2. `How it fits your day` — brief situation-level context, without explaining capabilities.
3. `What it can do` — a pure list of links to catalog entries.
4. `Settings that matter` — a pure list of links to rows or pages under `docs/reference/settings/`.
5. `When it goes wrong` — a pure list of links to the catalog entries that own the failure and its fix. A symptom index is planned; until it is published, do not link one.
6. `Get set up` — a short walkthrough using exact text breadcrumbs.

Under the three link-only headings, every nonblank line is a Markdown list item containing a link. A bold grouping label may prefix a list, but it may not carry behavioral prose. The next heading ends the block.

## Catalog anchors and slugs

Each feature entry is a `###` heading followed by one explicit HTML anchor on its own line, with a quoted id:

```markdown
### The paste didn't land — get the words back without re-dictating

<a id="the-paste-didnt-land-get-the-words-back"></a>
```

Slug rules:

1. Use a lowercase, ASCII, kebab-case problem or outcome phrase.
2. Freeze the slug when the entry is published. A heading may change; its slug may not.
3. Never reuse or renumber a slug.
4. If a slug must change, leave `<a id=old-slug></a>` as a compatibility stub and add the new anchor separately.
5. Keep retired features in the catalog and mark the retirement in the entry. Do not recycle their anchors.
6. Never link to GitHub’s generated heading slug. Link only to the explicit anchor.

Every catalog entry contains, in order:

- `The situation.`
- `What Handy does.`
- `Where.` — the breadcrumb, or a statement that no control exists. Platform scope is the _{Windows only}_ marker on this line.
- `Since.` — the version the behavior shipped in.

A provenance comment naming the source paths that can invalidate the claim is added entry by entry; `--touched` reports only the entries that carry one.

The heading names the reader’s problem or outcome, not the implementation mechanism.

## Reference allowances

The reference tree repeats a little more than other pages, because a lookup table has to be usable on its own:

- `docs/reference/shortcuts.md` may give one clause per bindable action in its `What it does` column. Anything longer belongs to a catalog entry and is linked instead.
- `docs/reference/glossary.md` may give one definitional sentence per term, followed by a `See` link to the entry that owns the behavior. No guarantees, limits, or defaults.
- `docs/reference/settings/*.md` may state a control's purpose, its interaction with named controls, and its shipped default. It does not restate the guarantee behind the control.

## Adding a feature

1. Verify the behavior against the source and delivery history. Do not infer a platform, default, or version.
2. Search `docs/features.md` for an existing entry. Extend that entry when it already owns the behavior.
3. If it is new, choose an unused frozen slug and add the explicit anchor plus the complete catalog template in the correct tool section.
4. Add exact settings breadcrumbs and an `Applies to.` line. Jumper and anchor behavior is Windows-only.
5. Add the provenance comment. List every source file whose change could make the claim stale.
6. Link to the entry from the relevant tool, learning-path, or reference page. Copy the heading exactly as the visible link text. An entry no page links to is unreachable in practice; give every new entry at least one inbound link.
7. Run the drift checker. Fix the document or catalog; do not weaken the checker to admit drift.

## Breadcrumb notation

Use text breadcrumbs instead of screenshots. The whole path is one inline-code span, with spaces around the U+203A separator:

```markdown
`Page › Tab › Group › Control = Value` _{annotation}_
```

Mechanical rules:

- Copy every page, tab, group, control, and option label character-for-character from the generated navigation map backed by `src/i18n/locales/en/translation.json`.
- Use `›`, never `>`, `->`, or `→`.
- Use at most four application segments: page, tab, group, control.
- The `Advanced` tabs are App, Transcription, Providers, MCP & CLI, History, and Post-processing. There is no Experimental tab and no Experimental Features control; never write either.
- Omit a group when its visible title equals the page title.
- Add ` = Value` only when the instruction tells the reader to choose that value. Split on the last `=`.
- Toggle values are `On` and `Off`. Chords are lowercase, `+`-joined, and contain no spaces.
- Put a label-less child control in a declared synthetic segment such as `[slot]`. The identifier must exist in the navigation map.
- Give the full path on its first mention on every page. Short label references may follow once the page has established context.
- Put gating outside the code span. The allowed annotations are _{Windows only}_, _{requires: Debug mode}_, _{requires: Post-processing enabled}_, and _{planned}_. Combine them with `; ` inside one pair of braces.
- Reserved non-settings roots are `Tray`, `Shortcut`, `CLI`, and `File`.
- Never put a breadcrumb in a heading or leave one as an unexplained bare line.

Valid examples:

```markdown
Set `Advanced › Transcription › Transcribe › Paste Method = Clipboard (Ctrl+V)`.

Set `Advanced › Transcription › Transcribe & Submit › Jump slot action on finish = Jump / deliver to slot` _{Windows only}_.
```

## Adding or changing a setting

1. Record the setting in the machine control map with its stable i18n key, `AppSettings` field, actual screen and group, render sites, catalog slug, and platform or visibility gate. Never infer the screen from the key namespace.
2. Add or update the row on the matching `docs/reference/settings/` page in on-screen order.
3. Use the exact rendered label as the row heading, an exact breadcrumb, an explicit shipped default, and a catalog link where one exists.
4. If a key renders in more than one place, make its copy true in every render site or split the key in code.
5. Treat the English string in `src/i18n/locales/en/translation.json` as the tooltip source. Draft the long-form reference first, then update the tooltip, then restamp or verify the docs cell.
6. Mark store-only, deprecated, reserved, and gated fields explicitly. Never document a reserved field as working.

## Drift enforcement

The documentation check is a release gate. Its required interface is `bun run docs:check`; the underlying checker verifies the following mechanically:

- Every Markdown file target and explicit `#anchor` resolves.
- Feature-link text equals the target catalog heading byte-for-byte.
- Link-only blocks contain no prose.
- Every catalog entry has its explicit unique anchor and its four required fields, and every Windows-only entry carries the marker on its `Where.` line.
- Every catalog entry is linked from at least one page outside `docs/features.md`.
- No page states or implies a released macOS or Linux build, and no page cites a version other than the current one.
- No anchor is duplicated; retired anchors remain reserved.
- README capability links resolve to live, non-retired entries.
- Tool-page headings do not duplicate catalog headings.
- Every `AppSettings` field appears in the control map or is explicitly flagged `no-ui`, `deprecated`, or `reserved`.
- Every documented tooltip key exists, every settings key has a reference row, and quoted English tooltip text equals the live locale string byte-for-byte.
- Breadcrumb segments and option values exist in the navigation map; gating annotations use the closed vocabulary.

`--fix` may repair catalog-link text and restamp tooltip quotations from their canonical sources. It must not generate prose, choose slugs, change platform scope, or invent missing claims. `--touched <files>` reports the catalog entries and tooltip keys whose provenance names changed source files so a maintainer can re-verify them.

Before a version bump:

1. Run `bun run docs:check` and require a clean result.
2. Run the touched-source report for the release changes and re-verify every reported claim.
3. Record which catalog entries and tooltip keys changed. If none changed, record `docs: none` with the reason.
4. Do not release with unresolved links, stale labels, unmapped settings, or unreviewed safety claims.
