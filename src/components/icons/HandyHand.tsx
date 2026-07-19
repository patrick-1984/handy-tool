/**
 * Generic brand glyph (hex badge + check — "a task, done"). Replaces the old
 * hand drawing; same component API so call sites are unchanged.
 */
const HandyHand = ({
  width,
  height,
}: {
  width?: number | string;
  height?: number | string;
}) => (
  <svg
    width={width || 24}
    height={height || 24}
    viewBox="0 0 24 24"
    fill="none"
    className="stroke-text"
    xmlns="http://www.w3.org/2000/svg"
  >
    <path
      d="M 7.5 4.2 L 16.5 4.2 L 21 12 L 16.5 19.8 L 7.5 19.8 L 3 12 Z"
      strokeWidth="1.9"
      strokeLinejoin="round"
    />
    <path
      d="M 8.2 12.4 L 11 15.2 L 16 8.6"
      strokeWidth="2.1"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  </svg>
);

export default HandyHand;
