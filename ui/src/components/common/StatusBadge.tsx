import { CheckCircle2, CircleDashed, CircleDot, CircleSlash, GitMerge, GitPullRequest, Loader2, XCircle } from "lucide-react";
import type { LucideIcon } from "lucide-react";

const STATUS: Record<string, { color: string; icon: LucideIcon; spin?: boolean }> = {
  succeeded: { color: "var(--color-status-success)", icon: CheckCircle2 },
  success: { color: "var(--color-status-success)", icon: CheckCircle2 },
  open: { color: "var(--color-status-success)", icon: CircleDot },
  merged: { color: "var(--color-status-merged)", icon: GitMerge },
  draft: { color: "var(--color-status-muted)", icon: GitPullRequest },
  running: { color: "var(--color-status-warning)", icon: Loader2, spin: true },
  queued: { color: "var(--color-status-muted)", icon: CircleDashed },
  pending: { color: "var(--color-status-muted)", icon: CircleDashed },
  failed: { color: "var(--color-status-error)", icon: XCircle },
  failure: { color: "var(--color-status-error)", icon: XCircle },
  closed: { color: "var(--color-status-error)", icon: CircleSlash },
  cancelled: { color: "var(--color-status-muted)", icon: CircleSlash },
  skipped: { color: "var(--color-status-muted)", icon: CircleSlash },
};

export default function StatusBadge({ status, label, iconOnly }: { status: string; label?: string; iconOnly?: boolean }) {
  const entry = STATUS[status] ?? { color: "var(--color-status-muted)", icon: CircleDashed };
  const Icon = entry.icon;

  if (iconOnly) {
    return <Icon className={entry.spin ? "h-4 w-4 shrink-0 animate-spin" : "h-4 w-4 shrink-0"} style={{ color: entry.color }} strokeWidth={2} />;
  }

  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-xs font-medium${label ? "" : " capitalize"}`}
      style={{ color: entry.color, borderColor: `color-mix(in srgb, ${entry.color} 30%, transparent)`, backgroundColor: `color-mix(in srgb, ${entry.color} 12%, transparent)` }}
    >
      <Icon className={entry.spin ? "h-3 w-3 animate-spin" : "h-3 w-3"} strokeWidth={2.5} />
      {label ?? status}
    </span>
  );
}
