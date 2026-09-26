import React from "react";

// "**text**" inside a line becomes bold.
const renderInline = (line: string): React.ReactNode[] =>
  line.split(/\*\*(.+?)\*\*/g).map((part, i) =>
    i % 2 === 1 ? (
      <strong key={i} className="font-semibold">
        {part}
      </strong>
    ) : (
      part
    ),
  );

/**
 * Renders a setting description. Plain text stays one paragraph — which is what
 * every translation still is — while the English texts add a little structure:
 * a blank line starts a new paragraph, lines starting with "- " form a bullet
 * list, and **text** is bold.
 */
export const RichText: React.FC<{ text: string; className?: string }> = ({
  text,
  className = "",
}) => {
  const nodes: React.ReactNode[] = [];
  for (const block of text.split(/\n\s*\n/)) {
    let bullets: string[] = [];
    const flushBullets = () => {
      if (bullets.length === 0) return;
      nodes.push(
        <ul key={nodes.length} className="list-disc ps-4 space-y-0.5">
          {bullets.map((item, i) => (
            <li key={i}>{renderInline(item)}</li>
          ))}
        </ul>,
      );
      bullets = [];
    };
    for (const line of block.split("\n")) {
      if (line.startsWith("- ")) {
        bullets.push(line.slice(2));
      } else if (line.trim()) {
        flushBullets();
        nodes.push(<p key={nodes.length}>{renderInline(line)}</p>);
      }
    }
    flushBullets();
  }
  return <div className={`space-y-1.5 ${className}`}>{nodes}</div>;
};
