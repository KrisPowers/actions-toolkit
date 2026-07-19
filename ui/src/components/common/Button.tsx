import { forwardRef } from "react";
import type { ButtonHTMLAttributes } from "react";
import { cn } from "../../lib/cn";

export type ButtonVariant = "primary" | "default" | "danger" | "danger-primary" | "invisible";
export type ButtonSize = "sm" | "md" | "icon";

const VARIANT_CLASSES: Record<ButtonVariant, string> = {
  primary: "border border-transparent bg-accent text-white hover:bg-accent-hover",
  default: "border border-neutral-800 bg-neutral-800/40 text-neutral-100 hover:border-neutral-700 hover:bg-neutral-800/70",
  danger:
    "border border-neutral-800 bg-neutral-800/40 text-[var(--color-status-error)] hover:border-transparent hover:bg-[var(--color-status-error)] hover:text-white",
  "danger-primary": "border border-transparent bg-[var(--color-status-error)] text-white hover:brightness-110",
  invisible: "border border-transparent text-neutral-400 hover:bg-neutral-800 hover:text-neutral-200",
};

const SIZE_CLASSES: Record<ButtonSize, string> = {
  sm: "gap-1.5 rounded-md px-2.5 py-1 text-xs",
  md: "gap-1.5 rounded-md px-3 py-1.5 text-sm",
  icon: "rounded-md p-1.5",
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
