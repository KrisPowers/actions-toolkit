import { cn } from "../../lib/cn";

export default function BrandMark({ size = 32, className }: { size?: number; className?: string }) {
  return (
    <div
      className={cn("flex shrink-0 items-center justify-center rounded-md bg-accent font-bold text-white", className)}
      style={{ width: size, height: size, fontSize: size * 0.45 }}
    >
      A
    </div>
  );
}
