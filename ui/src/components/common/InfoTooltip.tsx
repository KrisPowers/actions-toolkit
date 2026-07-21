import { useId, useState } from "react";
import type { ReactNode } from "react";
import { Info } from "lucide-react";
import { cn } from "../../lib/cn";

/**
 * A hover/focus-triggered "i" icon that reveals a clear instructional panel, instead of cramming
 * that explanation into the surrounding text. The visible glyph sits flush with adjacent body
 * text (no manual margin nudging needed at call sites) while `before:-inset-2` pads the actual
 * hit target out to a comfortable 32px square without affecting layout.
 */
export default function InfoTooltip({ text, className }: { text: ReactNode; className?: string }) {
  const [open, setOpen] = useState(false);
  const id = useId();

  return (
    <span className={cn("relative inline-flex", className)}>
      <button
        type="button"
        aria-describedby={id}
        aria-label="More info"
        onMouseEnter={() => setOpen(true)}
        onMouseLeave={() => setOpen(false)}
        onFocus={() => setOpen(true)}
        onBlur={() => setOpen(false)}
        onClick={() => setOpen((o) => !o)}
        className="relative flex h-4 w-4 shrink-0 items-center justify-center rounded-full text-neutral-500 before:absolute before:-inset-2 before:content-[''] hover:text-neutral-200"
      >
        <Info className="h-3.5 w-3.5" strokeWidth={2} />
      </button>
      {open && (
        <div
          id={id}
          role="tooltip"
          className="absolute left-1/2 top-full z-20 mt-2 w-80 max-w-[90vw] -translate-x-1/2 rounded-md border border-neutral-800 bg-neutral-950 p-3 text-xs leading-relaxed text-neutral-300 shadow-xl"
        >
          {text}
        </div>
      )}
    </span>
  );
}
