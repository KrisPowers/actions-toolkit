import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Download, Workflow } from "lucide-react";
import GithubMark from "../common/GithubMark";
import type { GithubWorkflowFile } from "../../api/types";
import { useImportGithubWorkflow } from "../../hooks/useWorkflows";

function GithubActionsTag() {
  return (
    <span
      title="Still runs on GitHub's own runners, not here"
      className="inline-flex shrink-0 items-center gap-1 rounded-full border border-neutral-800 px-2 py-0.5 text-[10px] font-medium text-neutral-500"
    >
      <GithubMark className="h-2.5 w-2.5" />
      GitHub Actions
    </span>
  );
}

// Workflow files GitHub still runs on its own hosted runners, listed alongside the workflows
// this app runs locally so "everything triggered by this repo" reads as one list. Each row can
// only be converted to run locally, not selected/edited/deleted like a local workflow.
export default function GithubWorkflowRows({
  repoId,
  files,
  isLoading,
}: {
  repoId: string;
  files: GithubWorkflowFile[] | undefined;
  isLoading: boolean;
}) {
  const importWorkflow = useImportGithubWorkflow(repoId);
  const navigate = useNavigate();
  const [importingPath, setImportingPath] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  if (!isLoading && (files ?? []).length === 0) return null;

  async function convert(path: string) {
    setImportingPath(path);
    setError(null);
    try {
      const workflow = await importWorkflow.mutateAsync(path);
      navigate(`/repos/${repoId}/workflows/${workflow.id}`);
    } catch (e) {
      setError((e as Error).message);
      setImportingPath(null);
    }
  }

  return (
    <>
      {isLoading && <div className="px-4 py-2.5 text-sm text-neutral-500">Loading GitHub workflows…</div>}
      {(files ?? []).map((f) => (
        <div key={f.path} className="flex items-center gap-2 px-4 py-2.5">
          <Workflow className="h-3.5 w-3.5 shrink-0 text-neutral-500" strokeWidth={2} />
          <span className="min-w-0 flex-1 truncate text-sm text-neutral-300" title={f.path}>
            {f.name}
          </span>
          <GithubActionsTag />
          <button
            type="button"
            onClick={() => convert(f.path)}
            disabled={importingPath === f.path}
            title="Convert to local runner"
            className="flex h-6 w-6 shrink-0 items-center justify-center rounded text-neutral-500 hover:bg-neutral-800 hover:text-neutral-200 disabled:opacity-50"
          >
            <Download className="h-3 w-3" strokeWidth={2} />
          </button>
        </div>
      ))}
      {error && <p className="px-4 py-2 text-xs text-[var(--color-status-error)]">{error}</p>}
    </>
  );
}
