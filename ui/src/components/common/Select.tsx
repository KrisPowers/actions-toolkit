import { forwardRef } from "react";
import type { SelectHTMLAttributes } from "react";
import { fieldClass } from "../../lib/fieldClass";

const Select = forwardRef<HTMLSelectElement, SelectHTMLAttributes<HTMLSelectElement>>(function Select({ className, ...props }, ref) {
  return <select ref={ref} className={fieldClass(className)} {...props} />;
});

export default Select;
