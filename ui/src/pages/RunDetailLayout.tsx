import { useState } from "react";
import { Outlet, useParams } from "react-router-dom";
import { AlertTriangle, Download, RotateCcw, XCircle } from "lucide-react";
import { useCancelRun, useRerun, useRun } from "../hooks/useRuns";
import StatusBadge from "../components/common/StatusBadge";
import Button from "../components/common/Button";
import PageHeader from "../components/common/PageHeader";
import RunDetailSidebar from "../components/runs/RunDetailSidebar";
import { exportRunReport } from "../lib/exportRun";
import type { RunTree } from "../api/types";

const ACTIVE_STATUSES = new Set(["queued", "running", "pending"]);

export interface RunDetailContext {
  tree: RunTree;
  runActive: boolean;
}

export default function RunDetailLayout() {
  const { repoId, runId } = useParams();
  const { data: tree } = useRun(runId);
  const cancel = useCancelRun();
  const rerun = useRerun();
  const [exporting, setExporting] = useState(false);
  const [exportError, setExportError] = useState<string | null>(null);

  if (!tree) return <p className="text-sm text-neutral-500">Loading…</p>;

  const runActive = ACTIVE_STATUSES.has(tree.run.status);

  async function handleExport() {
    setExporting(true);
    setExportError(null);
    try {
      await exportRunReport(tree as RunTree);
    } catch (e) {
      setExportError(e instanceof Error ? e.message : "failed to export run report");
    } finally {
      setExporting(false);
    }
  }

  return (
    <div className="flex h-full flex-col">
      <div className="pb-3">
        <PageHeader
          title={
            <span className="flex items-center gap-2">
              {tree.run.trigger_event}
              <StatusBadge status={tree.run.status} />
            </span>
          }
          backTo={`/repos/${tree.run.repo_id}/overview`}
          backLabel="Overview"
          actions={
            <>
              <Button variant="default" disabled={exporting} onClick={handleExport}>
                <Download className="h-3.5 w-3.5" strokeWidth={2} />
                {exporting ? "Exporting…" : "Download logs"}
              </Button>
              {runActive && (
                <Button variant="danger" onClick={() => cancel.mutate(tree.run.id)}>
                  <XCircle className="h-3.5 w-3.5" strokeWidth={2} />
                  Cancel
                </Button>
              )}
              {!runActive && (
                <Button variant="default" onClick={() => rerun.mutate(tree.run.id)}>
                  <RotateCcw className="h-3.5 w-3.5" strokeWidth={2} />
                  Re-run
                </Button>
              )}
            </>
          }
        />
        {exportError && <p className="mt-2 text-xs text-[var(--color-status-error)]">{exportError}</p>}
      </div>

      {tree.run.github_report_error && (
        <div className="mb-3 flex items-start gap-2 rounded-md border border-[var(--color-status-warning)]/30 bg-[var(--color-status-warning)]/5 px-3 py-2.5 text-sm text-[var(--color-status-warning)]">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" strokeWidth={2} />
          <p>
            This run's status couldn't be reported back to GitHub: <span className="font-mono text-xs">{tree.run.github_report_error}</span>
          </p>
        </div>
      )}

      <div className="grid min-h-0 flex-1 grid-cols-1 gap-6 md:grid-cols-[200px_1fr]">
        <RunDetailSidebar repoId={repoId as string} runId={tree.run.id} />
        <div className="flex min-h-0 min-w-0 flex-col">
          <Outlet context={{ tree, runActive } satisfies RunDetailContext} />
        </div>
      </div>
    </div>
  );
}
