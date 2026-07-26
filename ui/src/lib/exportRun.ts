import { runsApi } from "../api/runs";
import { runstatsApi } from "../api/runstats";
import type { RunLog, RunTree } from "../api/types";

function triggerDownload(filename: string, contents: string, mimeType: string) {
  const blob = new Blob([contents], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
}

/**
 * Fetches everything the run detail page's three panels (logs, backend, insights) show and
 * bundles it into one downloadable JSON file, so a run's console output and resource-usage
 * history survive past the bucket's TTL reaper without needing dashboard access.
 */
export async function exportRunReport(tree: RunTree): Promise<void> {
  const runId = tree.run.id;
  const [logs, topology, stats] = await Promise.all([
    runsApi.logs(runId),
    runstatsApi.topologyForRun(runId).catch(() => null),
    runstatsApi.statsForRun(runId).catch(() => null),
  ]);

  const logsByStep = new Map<string, RunLog[]>();
  for (const line of logs) {
    const existing = logsByStep.get(line.step_run_id);
    if (existing) existing.push(line);
    else logsByStep.set(line.step_run_id, [line]);
  }

  const jobs = tree.jobs.map((jt) => ({
    job: jt.job,
    steps: jt.steps.map((step) => ({
      step,
      console: (logsByStep.get(step.id) ?? []).map((line) => ({ ts: line.ts, stream: line.stream, message: line.message })),
    })),
  }));

  const report = {
    generated_at: new Date().toISOString(),
    run: tree.run,
    jobs,
    backend: { topology },
    insights: stats
      ? {
          cache_hits: stats.cache_hits,
          cache_misses: stats.cache_misses,
          assets_cached: stats.assets_cached,
          peak_cpu_percent: stats.peak_cpu_percent,
          peak_memory_bytes: stats.peak_memory_bytes,
          samples: stats.samples,
        }
      : null,
  };

  const shortId = runId.slice(0, 8);
  triggerDownload(`actions-toolkit-run-${shortId}-${tree.run.status}.json`, JSON.stringify(report, null, 2), "application/json");
}
