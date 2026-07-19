import { forwardRef } from "react";
import type { InputHTMLAttributes } from "react";
import { fieldClass } from "../../lib/fieldClass";

const Input = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(function Input({ className, ...props }, ref) {
  return <input ref={ref} className={fieldClass(className)} {...props} />;
});

export default Input;
