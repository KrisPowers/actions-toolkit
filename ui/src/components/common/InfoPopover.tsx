import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { HelpCircle } from "lucide-react";

interface InfoPopoverProps {
  label: string;
  children: ReactNode;
  align?: "left" | "right";
}

/**
 * Icon button that reveals a floating panel on hover or click (click makes it reachable on
 * touch devices and keeps it open for reading, hover keeps mouse users fast).
 */
export default function InfoPopover({ label, children, align = "left" }: InfoPopoverProps) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    function onPointerDown(e: PointerEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    }
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") setOpen(false);
    }
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  return (
    <div
      className="relative inline-flex"
      ref={ref}
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => setOpen(false)}
    >
      <button
        type="button"
        aria-label={label}
        aria-expanded={open}
        onClick={() => setOpen(true)}
        className="inline-flex h-5 w-5 items-center justify-center rounded-full text-neutral-500 hover:text-accent"
      >
        <HelpCircle className="h-4 w-4" strokeWidth={2} />
      </button>
      {open && (
        <div
          className={`absolute top-full z-40 mt-2 w-80 rounded-lg border border-neutral-800 bg-neutral-900 p-4 text-sm shadow-xl ${
            align === "right" ? "right-0" : "left-0"
          }`}
        >
          {children}
        </div>
      )}
    </div>
  );
}
