import { CheckCircle2, CircleDashed, CircleSlash, Clock, GitMerge, Loader2, XCircle } from "lucide-react";
import type { LucideIcon } from "lucide-react";

const STATUS: Record<string, { color: string; icon: LucideIcon; spin?: boolean }> = {
  succeeded: { color: "var(--color-status-success)", icon: CheckCircle2 },
  success: { color: "var(--color-status-success)", icon: CheckCircle2 },
  merged: { color: "var(--color-status-success)", icon: GitMerge },
  running: { color: "var(--color-status-info)", icon: Loader2, spin: true },
  queued: { color: "var(--color-status-warning)", icon: Clock },
  pending: { color: "var(--color-status-muted)", icon: CircleDashed },
  failed: { color: "var(--color-status-error)", icon: XCircle },
  failure: { color: "var(--color-status-error)", icon: XCircle },
  cancelled: { color: "var(--color-status-muted)", icon: CircleSlash },
  skipped: { color: "var(--color-status-muted)", icon: CircleSlash },
};

export default function StatusBadge({ status }: { status: string }) {
  const entry = STATUS[status] ?? { color: "var(--color-status-muted)", icon: CircleDashed };
  const Icon = entry.icon;
  return (
    <span
      className="inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-xs font-medium capitalize"
      style={{ color: entry.color, borderColor: `color-mix(in srgb, ${entry.color} 30%, transparent)`, backgroundColor: `color-mix(in srgb, ${entry.color} 12%, transparent)` }}
    >
      <Icon className={entry.spin ? "h-3 w-3 animate-spin" : "h-3 w-3"} strokeWidth={2.5} />
      {status}
    </span>
  );
}
