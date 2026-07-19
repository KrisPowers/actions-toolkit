import { forwardRef } from "react";
import type { TextareaHTMLAttributes } from "react";
import { fieldClass } from "../../lib/fieldClass";

const Textarea = forwardRef<HTMLTextAreaElement, TextareaHTMLAttributes<HTMLTextAreaElement>>(function Textarea({ className, ...props }, ref) {
  return <textarea ref={ref} className={fieldClass(className)} {...props} />;
});

export default Textarea;
