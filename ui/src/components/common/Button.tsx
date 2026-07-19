import { forwardRef } from "react";
import type { ButtonHTMLAttributes } from "react";
import { cn } from "../../lib/cn";

export type ButtonVariant = "primary" | "default" | "danger" | "danger-primary" | "invisible";
export type ButtonSize = "sm" | "md" | "icon";

// Anatomy mirrors Primer's actual btn classes: a hairline border plus a 1px inset shadow
// standing in for GitHub's subtle top-highlight/bottom-shadow gradient, and a dedicated
// green btn-primary token (--color-btn-primary) kept separate from the blue accent, which
// GitHub reserves for links/focus rings rather than primary actions.
const VARIANT_CLASSES: Record<ButtonVariant, string> = {
  primary: "border border-transparent bg-btn-primary text-btn-primary-fg shadow-[inset_0_1px_0_rgba(255,255,255,0.03)] hover:bg-btn-primary-hover",
  default:
    "border border-neutral-800 bg-neutral-800/40 text-neutral-100 shadow-[0_1px_0_rgba(27,31,36,0.04)] hover:border-neutral-700 hover:bg-neutral-800/70",
  danger:
    "border border-neutral-800 bg-neutral-800/40 text-[var(--color-status-error)] shadow-[0_1px_0_rgba(27,31,36,0.04)] hover:border-transparent hover:bg-[var(--color-status-error)] hover:text-white",
  "danger-primary": "border border-transparent bg-[var(--color-status-error)] text-white hover:brightness-110",
  invisible: "border border-transparent text-neutral-400 hover:bg-neutral-800 hover:text-neutral-200",
};

// Fixed heights (rather than padding-only sizing) so buttons line up with inputs/selects,
// which share the same h-8/h-7 baseline via fieldClass.
const SIZE_CLASSES: Record<ButtonSize, string> = {
  sm: "h-7 gap-1.5 rounded-md px-3 text-xs",
  md: "h-8 gap-1.5 rounded-md px-4 text-sm",
  icon: "h-8 w-8 rounded-md",
};

export function buttonClass(variant: ButtonVariant = "default", size: ButtonSize = "md", className?: string) {
  return cn(
    "inline-flex items-center justify-center font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed",
    VARIANT_CLASSES[variant],
    SIZE_CLASSES[size],
    className,
  );
}

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
}

const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button({ variant = "default", size = "md", className, ...props }, ref) {
  return <button ref={ref} type={props.type ?? "button"} className={buttonClass(variant, size, className)} {...props} />;
});

export default Button;
