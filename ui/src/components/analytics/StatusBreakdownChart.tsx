import { Bar, BarChart, CartesianGrid, Cell, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import type { StatusCount } from "../../api/analytics";

// Fixed status palette (never themed / never reused for series identity) so a status color
// always means the same run outcome across the app, matching StatusBadge.
const STATUS_COLOR: Record<string, string> = {
  succeeded: "#0ca30c",
  failed: "#e66767",
  cancelled: "#898781",
  running: "#3987e5",
  queued: "#c98500",
};

const GRIDLINE = "#2c2c2a";
const MUTED_INK = "#898781";

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
              contentStyle={{ background: "#1a1a19", border: "1px solid #2c2c2a", borderRadius: 6, fontSize: 12 }}
              labelStyle={{ color: "#c3c2b7" }}
              cursor={{ fill: "rgba(255,255,255,0.04)" }}
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
