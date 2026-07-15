import type { AnalyticsSummary } from "../../api/analytics";

const GOOD = "#0ca30c";
const CRITICAL = "#e66767";
const MUTED = "#898781";

function Tile({ label, value, color }: { label: string; value: string; color?: string }) {
  return (
    <div className="rounded-lg border border-neutral-800 bg-neutral-900 px-4 py-3">
      <div className="text-xs text-neutral-500">{label}</div>
      <div className="mt-1 text-2xl font-semibold tabular-nums" style={{ color: color ?? "#e6e8ee" }}>
        {value}
      </div>
    </div>
  );
}

/**
 * A single headline percentage plus its supporting counts is a stat-tile job, not a chart
 * (dataviz guidance: a single number doesn't need a plotted form).
 */
export default function SuccessRateChart({ summary }: { summary: AnalyticsSummary }) {
  return (
    <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
      <Tile label="Success rate" value={`${Math.round(summary.success_rate * 100)}%`} color={summary.success_rate >= 0.8 ? GOOD : summary.success_rate < 0.5 ? CRITICAL : undefined} />
      <Tile label="Total runs" value={String(summary.total_runs)} />
      <Tile label="Failed" value={String(summary.failed)} color={summary.failed > 0 ? CRITICAL : MUTED} />
      <Tile
        label="Avg duration"
        value={summary.avg_duration_seconds != null ? `${Math.round(summary.avg_duration_seconds)}s` : "—"}
      />
    </div>
  );
}
