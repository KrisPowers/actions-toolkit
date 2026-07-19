import { forwardRef } from "react";
import type { InputHTMLAttributes } from "react";
import { fieldClass } from "../../lib/fieldClass";
import { cn } from "../../lib/cn";

const Input = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(function Input({ className, ...props }, ref) {
  return <input ref={ref} className={fieldClass(cn("h-8", className))} {...props} />;
});

export default Input;
