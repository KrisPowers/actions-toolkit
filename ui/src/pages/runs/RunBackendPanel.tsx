import { useOutletContext } from "react-router-dom";
import Card from "../../components/common/Card";
import BackendTopology from "../../components/runs/BackendTopology";
import ResourceCharts from "../../components/runs/ResourceCharts";
import { useLiveStats } from "../../hooks/useLiveStats";
import { useRunStatsSummary, useRunTopology } from "../../hooks/useRunstats";
import type { RunDetailContext } from "../RunDetailLayout";

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="text-xs text-neutral-500">{label}</div>
      <div className="mt-0.5 text-sm text-neutral-200">{value}</div>
    </div>
  );
}

export default function RunBackendPanel() {
  const { tree, runActive } = useOutletContext<RunDetailContext>();
  const runId = tree.run.id;

  const { data: topology } = useRunTopology(runId, runActive);
  const { data: summary } = useRunStatsSummary(runId, runActive);
  const samples = useLiveStats(runId, runActive);

  const latestSample = [...samples].reverse().find((s) => s.host_cpu_percent != null);

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto">
      <Card className="grid grid-cols-2 gap-4 p-4 sm:grid-cols-4">
        <Stat label="Assets cached" value={String(summary?.assets_cached ?? 0)} />
        <Stat label="Cache hits" value={String(summary?.cache_hits ?? 0)} />
        <Stat label="Cache misses" value={String(summary?.cache_misses ?? 0)} />
        <Stat label="Peak CPU" value={summary?.peak_cpu_percent != null ? `${summary.peak_cpu_percent.toFixed(0)}%` : "—"} />
      </Card>

      <ResourceCharts samples={samples} />

      {latestSample && (
        <p className="text-xs text-neutral-600">
          Host context: this machine was at {latestSample.host_cpu_percent?.toFixed(0)}% CPU and{" "}
          {latestSample.host_memory_percent?.toFixed(0)}% memory as of the last sample — useful for spotting whether a
          spike came from this run or from something else running on the same host at the time.
        </p>
      )}

      <BackendTopology bucket={topology?.bucket ?? null} shells={topology?.shell ? [topology.shell] : []} samples={samples} />
    </div>
  );
}
