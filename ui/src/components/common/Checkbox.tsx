import { forwardRef } from "react";
import type { InputHTMLAttributes } from "react";
import { cn } from "../../lib/cn";

const Checkbox = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(function Checkbox({ className, ...props }, ref) {
  return <input ref={ref} type="checkbox" className={cn("accent-accent", className)} {...props} />;
});

export default Checkbox;
