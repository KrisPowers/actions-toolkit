const COLORS: Record<string, string> = {
  succeeded: "bg-emerald-500/15 text-emerald-400 border-emerald-500/30",
  success: "bg-emerald-500/15 text-emerald-400 border-emerald-500/30",
  running: "bg-blue-500/15 text-blue-400 border-blue-500/30",
  queued: "bg-amber-500/15 text-amber-400 border-amber-500/30",
  pending: "bg-neutral-500/15 text-neutral-400 border-neutral-500/30",
  failed: "bg-red-500/15 text-red-400 border-red-500/30",
  failure: "bg-red-500/15 text-red-400 border-red-500/30",
  cancelled: "bg-neutral-500/15 text-neutral-400 border-neutral-500/30",
  skipped: "bg-neutral-500/15 text-neutral-500 border-neutral-500/20",
};

export default function StatusBadge({ status }: { status: string }) {
  const cls = COLORS[status] ?? "bg-neutral-500/15 text-neutral-400 border-neutral-500/30";
  return (
    <span className={`inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-xs font-medium capitalize ${cls}`}>
      {status}
    </span>
  );
}
