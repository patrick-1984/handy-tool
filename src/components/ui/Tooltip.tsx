import React, { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

type TooltipPosition = "top" | "bottom";

interface TooltipCoords {
  top: number;
  left: number;
  width: number;
  arrowLeft: number;
  actualPosition: TooltipPosition;
}

interface TooltipProps {
  targetRef: React.RefObject<HTMLElement>;
  position?: TooltipPosition;
  children: React.ReactNode;
}

// Wide enough that a long description reads as a few short paragraphs rather
// than a narrow column; narrower only when the window itself is.
const TOOLTIP_WIDTH = 340;
const VIEWPORT_PADDING = 12;
const GAP = 8;
const ARROW_MARGIN = 12;
const DEFAULT_HEIGHT = 60;

export const Tooltip: React.FC<TooltipProps> = ({
  targetRef,
  position = "top",
  children,
}) => {
  const [coords, setCoords] = useState<TooltipCoords | null>(null);
  const tooltipRef = useRef<HTMLDivElement>(null);

  const updatePosition = useCallback(() => {
    if (!targetRef.current) return;

    const targetRect = targetRef.current.getBoundingClientRect();
    const tooltipHeight = tooltipRef.current?.offsetHeight || DEFAULT_HEIGHT;

    let actualPosition = position;
    let top: number;

    if (position === "top") {
      const spaceAbove = targetRect.top - tooltipHeight - GAP;
      if (spaceAbove < VIEWPORT_PADDING) {
        actualPosition = "bottom";
        top = targetRect.bottom + GAP;
      } else {
        top = targetRect.top - GAP - tooltipHeight;
      }
    } else {
      const spaceBelow =
        window.innerHeight - targetRect.bottom - tooltipHeight - GAP;
      if (spaceBelow < VIEWPORT_PADDING) {
        actualPosition = "top";
        top = targetRect.top - GAP - tooltipHeight;
      } else {
        top = targetRect.bottom + GAP;
      }
    }

    const width = Math.min(
      TOOLTIP_WIDTH,
      window.innerWidth - 2 * VIEWPORT_PADDING,
    );
    const targetCenter = targetRect.left + targetRect.width / 2;
    let left = targetCenter - width / 2;

    if (left < VIEWPORT_PADDING) {
      left = VIEWPORT_PADDING;
    } else if (left + width > window.innerWidth - VIEWPORT_PADDING) {
      left = window.innerWidth - width - VIEWPORT_PADDING;
    }

    const arrowLeft = Math.min(
      Math.max(targetCenter - left, ARROW_MARGIN),
      width - ARROW_MARGIN,
    );

    setCoords({ top, left, width, arrowLeft, actualPosition });
  }, [targetRef, position]);

  useEffect(() => {
    updatePosition();

    window.addEventListener("scroll", updatePosition, true);
    window.addEventListener("resize", updatePosition);

    return () => {
      window.removeEventListener("scroll", updatePosition, true);
      window.removeEventListener("resize", updatePosition);
    };
  }, [updatePosition]);

  const arrowClasses =
    coords?.actualPosition === "top" ? "top-full" : "bottom-full rotate-180";

  return createPortal(
    <div
      ref={tooltipRef}
      style={{
        position: "fixed",
        top: coords?.top ?? -9999,
        left: coords?.left ?? -9999,
        width: coords?.width ?? TOOLTIP_WIDTH,
        zIndex: 9999,
        opacity: coords ? 1 : 0,
      }}
      className="px-3 py-2 bg-background border border-mid-gray/80 rounded-lg shadow-lg whitespace-normal transition-opacity duration-150"
    >
      {children}
      <div
        style={{ left: coords?.arrowLeft ?? 0 }}
        className={`absolute ${arrowClasses} transform -translate-x-1/2 w-0 h-0 border-l-[6px] border-r-[6px] border-t-[6px] border-l-transparent border-r-transparent border-t-mid-gray/80`}
      />
    </div>,
    document.body,
  );
};
