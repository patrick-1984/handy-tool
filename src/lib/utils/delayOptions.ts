import type { useTranslation } from "react-i18next";

/**
 * The canonical delay scale shared by every delay dropdown: 100 ms steps from
 * 100 up to 1000, then 500 ms steps to 2000. "Off" is prepended separately as
 * the `none` value.
 *
 * The grid is deliberately uniform: no 250. It used to be offered because it
 * was the shipped default of `jumper_paste_delay` / `jumper_submit_delay`, but
 * those defaults are now 300 (local) and 600 (remote), so nothing on the grid
 * is unreachable. `Ms250` still EXISTS as a Rust variant and is still honored —
 * a settings store written before this change keeps working, and
 * `buildDelayOptions` re-inserts that value in sorted position so the dropdown
 * shows it rather than collapsing to "Select an option…".
 *
 * Every value string here MUST exist as a Rust enum variant (serde
 * `rename_all = "snake_case"` turns `Ms1500` into `"ms1500"`) AND as an arm in
 * the matching `parse_*_delay` fn in `src-tauri/src/shortcut/mod.rs`. Those
 * parsers warn and fall back to the default instead of erroring, so a value
 * that exists only here writes the WRONG setting behind a green UI.
 */
export const DELAY_STEPS_MS = [
  100, 200, 300, 400, 500, 600, 700, 800, 900, 1000, 1500, 2000,
] as const;

const labelFor = (
  t: ReturnType<typeof useTranslation>["t"],
  nf: Intl.NumberFormat,
  ms: number,
): string =>
  ms < 1000
    ? t("common.delay.ms", { ms: nf.format(ms) })
    : t("common.delay.seconds", { seconds: nf.format(ms / 1000) });

/** Parses a `msNNNN` wire value back to its millisecond number, else null. */
const msFromValue = (value: string): number | null => {
  const match = /^ms(\d+)$/.exec(value);
  if (!match) return null;
  const ms = Number(match[1]);
  return Number.isFinite(ms) ? ms : null;
};

/**
 * Builds the option list for a delay dropdown.
 *
 * `current` is the value the setting currently holds. When it is not on the
 * canonical scale — a legacy `ms2500` / `ms5000` clipboard-restore value that
 * predates the 2 s ceiling — it is inserted in sorted position rather than
 * dropped. Those values are still honoured by the backend, and `Dropdown`
 * renders `options.find(o => o.value === selectedValue)?.label` — so omitting
 * one would show the user "Select an option…" over a setting that is in fact
 * active, and silently lose it on the next edit.
 */
export const buildDelayOptions = (
  t: ReturnType<typeof useTranslation>["t"],
  language: string,
  current?: string | null,
): { value: string; label: string }[] => {
  const nf = new Intl.NumberFormat(language, { maximumFractionDigits: 1 });

  const steps: number[] = [...DELAY_STEPS_MS];
  const currentMs = current ? msFromValue(current) : null;
  if (currentMs !== null && !steps.includes(currentMs)) {
    steps.push(currentMs);
    steps.sort((a, b) => a - b);
  }

  return [
    { value: "none", label: t("common.delay.off") },
    ...steps.map((ms) => ({ value: `ms${ms}`, label: labelFor(t, nf, ms) })),
  ];
};
