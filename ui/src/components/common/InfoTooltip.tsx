import { useId, useState } from "react";
import type { ReactNode } from "react";
import { Info } from "lucide-react";
import { cn } from "../../lib/cn";

/**
 * A hover/focus-triggered "i" icon that reveals a clear instructional panel, instead of cramming
 * that explanation into the surrounding text. Sized well above a bare icon (32px hit target, 18px
 * glyph) so it's actually easy to hover/tap, not a barely-visible dot.
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
        className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full text-neutral-500 hover:bg-neutral-800 hover:text-neutral-200"
      >
        <Info className="h-[18px] w-[18px]" strokeWidth={2} />
      </button>
      {open && (
        <div
          id={id}
          role="tooltip"
          className="absolute left-1/2 top-full z-20 mt-1 w-80 max-w-[90vw] -translate-x-1/2 rounded-md border border-neutral-800 bg-neutral-950 p-3 text-xs leading-relaxed text-neutral-300 shadow-xl"
        >
          {text}
        </div>
      )}
    </span>
  );
}
