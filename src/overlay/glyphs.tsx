// The pill's own glyphs, shared with the previews of the Appearance setup.

// The T button's letter, drawn: a font's T lands up to a screen pixel off
// centre in the circle (pixel snapping at 125% scaling), this one never does.
export const TGlyph = () => (
  <svg width="8" height="10" viewBox="0 0 8 10" aria-hidden>
    <rect width="8" height="2.2" rx="0.3" fill="currentColor" />
    <rect x="2.9" width="2.2" height="10" rx="0.3" fill="currentColor" />
  </svg>
);

// The pause and play glyphs, filled (lucide's are outlines).
export const PauseGlyph = () => (
  <svg width="12" height="12" viewBox="0 0 24 24" aria-hidden>
    <rect x="6" y="4" width="4" height="16" rx="1" fill="currentColor" />
    <rect x="14" y="4" width="4" height="16" rx="1" fill="currentColor" />
  </svg>
);
export const PlayGlyph = () => (
  <svg width="12" height="12" viewBox="0 0 24 24" aria-hidden>
    <path d="M6 3l14 9-14 9z" fill="currentColor" />
  </svg>
);
