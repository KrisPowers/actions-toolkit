import type { HTMLAttributes } from "react";
import { cn } from "../../lib/cn";

// GitHub's actual card radius is 6px (Primer's --borderRadius-medium), the same as its buttons
// and inputs, not the 8px rounded-lg Tailwind default.
export function cardClass(className?: string) {
  return cn("rounded-md border border-neutral-800 bg-neutral-900", className);
}

export function listCardClass(className?: string) {
  return cardClass(cn("divide-y divide-neutral-800", className));
}

// Row padding used inside a listCardClass container, exported so Link/NavLink rows that
// can't use the Card component directly (they need to be an <a>, not a <div>) stay consistent.
export function listRowClass(className?: string) {
  return cn("flex items-center justify-between px-4 py-3", className);
}

export default function Card({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return <div className={cardClass(className)} {...props} />;
}
