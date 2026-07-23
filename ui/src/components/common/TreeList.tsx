import type { ReactNode } from "react";
import { Children } from "react";
import { cn } from "../../lib/cn";

const LINE_BG = "bg-neutral-800";
const LINE_BORDER = "border-neutral-800";

/**
 * Vertical connector line down the left edge of a list of nested items, each with a short
 * horizontal branch into it. The last item's trunk stops short and rounds into its branch
 * instead of running past it, so the tree reads as terminating there rather than continuing off
 * into empty space.
 */
export default function TreeList({ children, className }: { children: ReactNode; className?: string }) {
  const items = Children.toArray(children);
  return (
    <div className={cn("flex flex-col gap-3", className)}>
      {items.map((child, i) => {
        const isLast = i === items.length - 1;
        return (
          <div key={i} className="relative pl-5">
            {isLast ? (
              <span aria-hidden className={cn("absolute left-0 top-0 h-4 w-3 rounded-bl-md border-b border-l", LINE_BORDER)} />
            ) : (
              <>
                <span aria-hidden className={cn("absolute left-0 top-0 h-full w-px", LINE_BG)} />
                <span aria-hidden className={cn("absolute left-0 top-4 h-px w-3", LINE_BG)} />
              </>
            )}
            {child}
          </div>
        );
      })}
    </div>
  );
}
