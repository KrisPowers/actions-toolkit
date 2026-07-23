import { CartesianGrid, Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import type { ResourceSample } from "../../api/types";

const CPU_LINE = "var(--color-status-info)";
const MEMORY_LINE = "var(--color-status-warning)";
const GRIDLINE = "var(--color-neutral-800)";
const MUTED_INK = "var(--color-neutral-500)";

function chartData(samples: ResourceSample[], subjectType: "shell", key: "cpu_percent" | "memory_bytes") {
  return samples
    .filter((s) => s.subject_type === subjectType)
    .sort((a, b) => a.ts.localeCompare(b.ts))
    .map((s) => ({
      time: new Date(s.ts).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" }),
      value: s[key] == null ? null : key === "memory_bytes" ? Math.round((s[key] as number) / (1024 * 1024)) : Math.round(s[key] as number),
    }));
}

function Chart({ title, data, color, unit }: { title: string; data: { time: string; value: number | null }[]; color: string; unit: string }) {
  return (
    <div className="rounded-lg border border-neutral-800 bg-neutral-900 p-4">
      <div className="text-sm font-medium text-neutral-200">{title}</div>
      <div className="mt-3 h-48">
        <ResponsiveContainer width="100%" height="100%">
          <LineChart data={data} margin={{ top: 4, right: 8, left: 0, bottom: 0 }}>
            <CartesianGrid stroke={GRIDLINE} vertical={false} />
            <XAxis dataKey="time" stroke={MUTED_INK} fontSize={10} tickLine={false} axisLine={{ stroke: GRIDLINE }} minTickGap={40} />
            <YAxis stroke={MUTED_INK} fontSize={11} tickLine={false} axisLine={false} width={40} unit={unit} />
            <Tooltip
              contentStyle={{ background: "var(--color-neutral-900)", border: "1px solid var(--color-neutral-800)", borderRadius: 6, fontSize: 12 }}
              labelStyle={{ color: "var(--color-neutral-300)" }}
            />
            <Line type="monotone" dataKey="value" name={title} stroke={color} strokeWidth={2} dot={false} connectNulls isAnimationActive={false} />
          </LineChart>
        </ResponsiveContainer>
      </div>
      {data.length === 0 && <p className="mt-2 text-xs text-neutral-600">No samples yet.</p>}
    </div>
  );
}

/** This shell's own CPU%/memory over the run's lifetime, updating live while the run is active. */
export default function ResourceCharts({ samples }: { samples: ResourceSample[] }) {
  return (
    <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
      <Chart title="Shell CPU usage" data={chartData(samples, "shell", "cpu_percent")} color={CPU_LINE} unit="%" />
      <Chart title="Shell memory usage" data={chartData(samples, "shell", "memory_bytes")} color={MEMORY_LINE} unit="MB" />
    </div>
  );
}
