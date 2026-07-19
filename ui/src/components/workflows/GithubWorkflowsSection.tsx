import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Download } from "lucide-react";
import GithubMark from "../common/GithubMark";
import Button from "../common/Button";
import { useGithubWorkflows, useImportGithubWorkflow } from "../../hooks/useWorkflows";

export default function GithubWorkflowsSection({ repoId }: { repoId: string }) {
  const { data: files, isLoading } = useGithubWorkflows(repoId);
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
    <div className="mt-8">
      <div className="flex items-center gap-2">
        <GithubMark className="h-4 w-4 text-neutral-500" />
        <h2 className="text-sm font-semibold text-neutral-200">Running on GitHub Actions</h2>
      </div>
      <p className="mt-1 text-xs text-neutral-500">
        These still run on GitHub's own runners, not here. Convert one to run it locally instead.
      </p>

      {error && <p className="mt-2 text-xs text-[var(--color-status-error)]">{error}</p>}

      <div className="mt-2 divide-y divide-neutral-800 rounded-lg border border-neutral-800 bg-neutral-900">
        {isLoading && <div className="px-4 py-4 text-sm text-neutral-500">Loading…</div>}
        {(files ?? []).map((f) => (
          <div key={f.path} className="flex items-center justify-between px-4 py-3">
            <div>
              <div className="text-sm font-medium text-neutral-100">{f.name}</div>
              <div className="mt-0.5 font-mono text-xs text-neutral-500">{f.path}</div>
            </div>
            <Button variant="default" size="sm" onClick={() => convert(f.path)} disabled={importingPath === f.path}>
              <Download className="h-3 w-3" strokeWidth={2} />
              {importingPath === f.path ? "Converting…" : "Convert to local runner"}
            </Button>
          </div>
        ))}
      </div>
    </div>
  );
}
