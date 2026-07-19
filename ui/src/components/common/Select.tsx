import { forwardRef } from "react";
import type { SelectHTMLAttributes } from "react";
import { fieldClass } from "../../lib/fieldClass";
import { cn } from "../../lib/cn";

const Select = forwardRef<HTMLSelectElement, SelectHTMLAttributes<HTMLSelectElement>>(function Select({ className, ...props }, ref) {
  return <select ref={ref} className={fieldClass(cn("h-8", className))} {...props} />;
});

export default Select;
