import { cn } from "./cn";

export function fieldClass(className?: string) {
  return cn(
    "rounded-md border border-neutral-700 bg-neutral-950 px-2.5 py-1.5 text-sm text-neutral-100 outline-none transition-shadow",
    "focus:border-accent focus:ring-2 focus:ring-accent/30",
    className,
  );
}
