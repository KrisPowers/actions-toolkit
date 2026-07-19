import type { MouseEvent, ReactNode } from "react";
import { cn } from "../../lib/cn";

export default function Modal({
  open,
  onClose,
  children,
  className = "max-w-sm",
}: {
  open: boolean;
  onClose: () => void;
  children: ReactNode;
  className?: string;
}) {
  if (!open) return null;

  function stopPropagation(e: MouseEvent) {
    e.stopPropagation();
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4 transition-opacity duration-150 starting:opacity-0"
      onClick={onClose}
    >
      <div
        className={cn(
          "w-full rounded-lg border border-neutral-800 bg-neutral-900 p-5 shadow-xl transition-all duration-150 starting:scale-95 starting:opacity-0",
          className,
        )}
        onClick={stopPropagation}
      >
        {children}
      </div>
    </div>
  );
}
