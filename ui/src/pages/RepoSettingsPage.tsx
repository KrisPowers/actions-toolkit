import { useRef, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { AlertTriangle, CheckCircle2, Download, RefreshCw, Trash2, Upload, Webhook, XCircle } from "lucide-react";
import { useDeleteRepo, useRepo, useTestRepoConnection } from "../hooks/useRepos";
import { useCreateWorkflow, useWorkflows } from "../hooks/useWorkflows";
import ConfirmDialog from "../components/common/ConfirmDialog";
import GithubTokenHelp from "../components/settings/GithubTokenHelp";
import GithubMark from "../components/common/GithubMark";
import { workflowsApi } from "../api/workflows";

function nameFromYaml(text: string, fallback: string): string {
  const match = text.match(/^name:\s*(.+)$/m);
  const raw = match?.[1]?.trim().replace(/^["']|["']$/g, "");
  return raw || fallback;
}

interface ImportResult {
  fileName: string;
  ok: boolean;
  message: string;
}

export default function RepoSettingsPage() {
  const { repoId } = useParams();
  const { data: repo } = useRepo(repoId);
  const { data: workflows } = useWorkflows(repoId);
  const testConnection = useTestRepoConnection();
  const deleteRepo = useDeleteRepo();
  const createWorkflow = useCreateWorkflow(repoId as string);
  const navigate = useNavigate();
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [importing, setImporting] = useState(false);
  const [importResults, setImportResults] = useState<ImportResult[]>([]);

  if (!repo) return null;

  async function handleImport(e: React.ChangeEvent<HTMLInputElement>) {
    const files = Array.from(e.target.files ?? []);
    e.target.value = "";
    if (files.length === 0) return;

    setImporting(true);
    const results: ImportResult[] = [];
    for (const file of files) {
      try {
        const text = await file.text();
        const fallbackName = file.name.replace(/\.ya?ml$/i, "");
        await createWorkflow.mutateAsync({
          name: nameFromYaml(text, fallbackName),
          yaml_source: text,
          file_path: file.name,
        });
        results.push({ fileName: file.name, ok: true, message: "imported" });
      } catch (err) {
        results.push({ fileName: file.name, ok: false, message: (err as Error).message });
      }
    }
    setImporting(false);
    setImportResults(results);
  }

  return (
    <div className="max-w-6xl">
      <h1 className="text-lg font-semibold text-neutral-100">
        {repo.owner}/{repo.name} settings
      </h1>

      <div className="mt-5 grid grid-cols-1 gap-5 xl:grid-cols-2">
        <div className="rounded-lg border border-neutral-800 bg-neutral-900 p-5">
          <div className="flex items-center gap-2">
            <Webhook className="h-4 w-4 text-neutral-500" strokeWidth={2} />
            <div className="text-sm font-medium text-neutral-200">Webhook</div>
          </div>
          <code className="mt-2 block break-all rounded bg-neutral-950 px-2 py-1 text-xs text-neutral-400">{repo.webhook_url}</code>
          <p className="mt-2 text-xs text-neutral-600">
            The webhook secret was shown once when this repo was connected. Disconnect and reconnect for a new one.
          </p>

          <div className="mt-5 flex items-center gap-2 border-t border-neutral-800 pt-4">
            <GithubMark className="h-4 w-4 text-neutral-500" />
            <div className="text-sm font-medium text-neutral-200">GitHub access</div>
            <GithubTokenHelp />
          </div>
          <p className="mt-1 text-xs text-neutral-500">
            Uses the account-wide token in{" "}
            <Link to="/settings" className="text-accent hover:underline">
              Settings
            </Link>
            .
          </p>

          <div className="mt-4 border-t border-neutral-800 pt-4">
            <button
              type="button"
              onClick={() => testConnection.mutate(repo.id)}
              disabled={testConnection.isPending}
              className="inline-flex items-center gap-1.5 rounded-md border border-neutral-700 px-3 py-1.5 text-sm text-neutral-200 hover:bg-neutral-800"
            >
              <RefreshCw className={`h-3.5 w-3.5 ${testConnection.isPending ? "animate-spin" : ""}`} strokeWidth={2} />
              {testConnection.isPending ? "Testing…" : "Test connection"}
            </button>
            {testConnection.data && (
              <p
                className="mt-2 text-sm"
                style={{ color: testConnection.data.ok ? "var(--color-status-success)" : "var(--color-status-error)" }}
              >
                {testConnection.data.message}
              </p>
            )}
          </div>
        </div>

        <div className="rounded-lg border border-neutral-800 bg-neutral-900 p-5">
          <div className="flex items-center gap-2">
            <Download className="h-4 w-4 text-neutral-500" strokeWidth={2} />
            <div className="text-sm font-medium text-neutral-200">Import &amp; export</div>
          </div>
          <p className="mt-1 text-xs text-neutral-500">
            Export the raw workflow YAML files this tool has saved, or upload <code>.yml</code> files to create new
            local-runner workflows.
          </p>

          <a
            href={workflowsApi.exportAllUrl(repo.id)}
            className={`mt-4 inline-flex items-center gap-1.5 rounded-md border border-neutral-700 px-3 py-1.5 text-sm text-neutral-200 hover:bg-neutral-800 ${
              (workflows ?? []).length === 0 ? "pointer-events-none opacity-50" : ""
            }`}
          >
            <Download className="h-3.5 w-3.5" strokeWidth={2} />
            Export all workflows (.zip)
          </a>

          <div className="mt-5 border-t border-neutral-800 pt-4">
            <input
              ref={fileInputRef}
              type="file"
              accept=".yml,.yaml"
              multiple
              className="hidden"
              onChange={handleImport}
            />
            <button
              type="button"
              onClick={() => fileInputRef.current?.click()}
              disabled={importing}
              className="inline-flex items-center gap-1.5 rounded-md border border-neutral-700 px-3 py-1.5 text-sm text-neutral-200 hover:bg-neutral-800 disabled:opacity-60"
            >
              <Upload className="h-3.5 w-3.5" strokeWidth={2} />
              {importing ? "Importing…" : "Import workflow files"}
            </button>

            {importResults.length > 0 && (
              <ul className="mt-3 flex flex-col gap-1">
                {importResults.map((r, i) => (
                  <li key={`${r.fileName}-${i}`} className="flex items-center gap-1.5 text-xs">
                    {r.ok ? (
                      <CheckCircle2 className="h-3.5 w-3.5 shrink-0 text-[var(--color-status-success)]" strokeWidth={2} />
                    ) : (
                      <XCircle className="h-3.5 w-3.5 shrink-0 text-[var(--color-status-error)]" strokeWidth={2} />
                    )}
                    <span className="text-neutral-300">{r.fileName}</span>
                    {!r.ok && <span className="text-neutral-500">— {r.message}</span>}
                  </li>
                ))}
              </ul>
            )}
          </div>
        </div>

        <div className="rounded-lg border border-[var(--color-status-error)]/30 bg-[var(--color-status-error)]/5 p-5 xl:col-span-2">
          <div className="flex items-center gap-2 text-[var(--color-status-error)]">
            <AlertTriangle className="h-4 w-4" strokeWidth={2} />
            <div className="text-sm font-medium">Danger zone</div>
          </div>
          <p className="mt-2 text-xs text-neutral-500">Disconnecting removes this repo, its workflows, and run history.</p>
          <button
            type="button"
            onClick={() => setConfirmDelete(true)}
            className="mt-3 inline-flex items-center gap-1.5 rounded-md border border-[var(--color-status-error)]/40 px-3 py-1.5 text-sm text-[var(--color-status-error)] hover:bg-[var(--color-status-error)]/10"
          >
            <Trash2 className="h-3.5 w-3.5" strokeWidth={2} />
            Disconnect repo
          </button>
        </div>
      </div>

      <ConfirmDialog
        open={confirmDelete}
        title="Disconnect repo"
        message={`This removes ${repo.owner}/${repo.name} and all of its workflows and run history. This cannot be undone.`}
        confirmLabel="Disconnect"
        danger
        onCancel={() => setConfirmDelete(false)}
        onConfirm={() => deleteRepo.mutate(repo.id, { onSuccess: () => navigate("/repos") })}
      />
    </div>
  );
}
