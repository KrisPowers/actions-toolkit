import { CartesianGrid, Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import type { DurationTrendPoint } from "../../api/analytics";

const SERIES_BLUE = "#3987e5";
const GRIDLINE = "#2c2c2a";
const MUTED_INK = "#898781";

export default function DurationTrendChart({ points }: { points: DurationTrendPoint[] }) {
  const data = points.map((p) => ({
    day: p.day.slice(5),
    seconds: p.avg_duration_seconds != null ? Math.round(p.avg_duration_seconds) : null,
  }));

  return (
    <div className="rounded-lg border border-neutral-800 bg-neutral-900 p-4">
      <div className="text-sm font-medium text-neutral-200">Average run duration</div>
      <div className="mt-3 h-56">
        <ResponsiveContainer width="100%" height="100%">
          <LineChart data={data} margin={{ top: 4, right: 8, left: 0, bottom: 0 }}>
            <CartesianGrid stroke={GRIDLINE} vertical={false} />
            <XAxis dataKey="day" stroke={MUTED_INK} fontSize={11} tickLine={false} axisLine={{ stroke: GRIDLINE }} />
            <YAxis stroke={MUTED_INK} fontSize={11} tickLine={false} axisLine={false} width={36} unit="s" />
            <Tooltip
              contentStyle={{ background: "#1a1a19", border: "1px solid #2c2c2a", borderRadius: 6, fontSize: 12 }}
              labelStyle={{ color: "#c3c2b7" }}
            />
            <Line type="monotone" dataKey="seconds" name="avg seconds" stroke={SERIES_BLUE} strokeWidth={2} dot={{ r: 3 }} connectNulls />
          </LineChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}
