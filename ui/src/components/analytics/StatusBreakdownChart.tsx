import { Bar, BarChart, CartesianGrid, Cell, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import type { StatusCount } from "../../api/analytics";

// Fixed status palette (never themed / never reused for series identity) so a status color
// always means the same run outcome across the app, matching StatusBadge.
const STATUS_COLOR: Record<string, string> = {
  succeeded: "var(--color-status-success)",
  failed: "var(--color-status-error)",
  cancelled: "var(--color-status-muted)",
  running: "var(--color-status-info)",
  queued: "var(--color-status-warning)",
};

const GRIDLINE = "var(--color-neutral-800)";
const MUTED_INK = "var(--color-neutral-500)";

export default function StatusBreakdownChart({ counts }: { counts: StatusCount[] }) {
  const data = counts.map((c) => ({ status: c.status, count: c.count }));

  return (
    <div className="rounded-lg border border-neutral-800 bg-neutral-900 p-4">
      <div className="text-sm font-medium text-neutral-200">Runs by status</div>
      <div className="mt-3 h-56">
        <ResponsiveContainer width="100%" height="100%">
          <BarChart data={data} layout="vertical" margin={{ top: 4, right: 16, left: 8, bottom: 0 }}>
            <CartesianGrid stroke={GRIDLINE} horizontal={false} />
            <XAxis type="number" stroke={MUTED_INK} fontSize={11} tickLine={false} axisLine={{ stroke: GRIDLINE }} allowDecimals={false} />
            <YAxis type="category" dataKey="status" stroke={MUTED_INK} fontSize={11} tickLine={false} axisLine={false} width={80} />
            <Tooltip
              contentStyle={{ background: "var(--color-neutral-900)", border: "1px solid var(--color-neutral-800)", borderRadius: 6, fontSize: 12 }}
              labelStyle={{ color: "var(--color-neutral-300)" }}
              cursor={{ fill: "rgba(128,128,128,0.08)" }}
            />
            <Bar dataKey="count" radius={[0, 4, 4, 0]} maxBarSize={18}>
              {data.map((d) => (
                <Cell key={d.status} fill={STATUS_COLOR[d.status] ?? MUTED_INK} />
              ))}
            </Bar>
          </BarChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}
