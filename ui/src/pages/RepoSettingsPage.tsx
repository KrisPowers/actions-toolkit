import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { useDeleteRepo, useRepo, useTestRepoConnection } from "../hooks/useRepos";
import ConfirmDialog from "../components/common/ConfirmDialog";

export default function RepoSettingsPage() {
  const { repoId } = useParams();
  const { data: repo } = useRepo(repoId);
  const testConnection = useTestRepoConnection();
  const deleteRepo = useDeleteRepo();
  const navigate = useNavigate();
  const [confirmDelete, setConfirmDelete] = useState(false);

  if (!repo) return null;

  return (
    <div className="max-w-lg">
      <h1 className="text-lg font-semibold text-neutral-100">
        {repo.owner}/{repo.name} settings
      </h1>

      <div className="mt-5 rounded-lg border border-neutral-800 bg-neutral-900 p-5">
        <div className="text-sm font-medium text-neutral-200">Webhook</div>
        <code className="mt-1 block break-all rounded bg-neutral-950 px-2 py-1 text-xs text-neutral-400">{repo.webhook_url}</code>
        <p className="mt-1 text-xs text-neutral-600">
          The webhook secret was shown once when this repo was connected. Disconnect and reconnect to generate a new one.
        </p>

        <div className="mt-4 text-sm font-medium text-neutral-200">GitHub access</div>
        <p className="mt-1 text-xs text-neutral-500">
          Uses the account-wide GitHub token configured in{" "}
          <Link to="/settings" className="text-accent hover:underline">
            Settings
          </Link>
          .
        </p>

        <div className="mt-5 border-t border-neutral-800 pt-4">
          <button
            type="button"
            onClick={() => testConnection.mutate(repo.id)}
            disabled={testConnection.isPending}
            className="rounded-md border border-neutral-700 px-3 py-1.5 text-sm text-neutral-200 hover:bg-neutral-800"
          >
            {testConnection.isPending ? "Testing…" : "Test connection"}
          </button>
          {testConnection.data && (
            <p className={`mt-2 text-sm ${testConnection.data.ok ? "text-emerald-400" : "text-red-400"}`}>
              {testConnection.data.message}
            </p>
          )}
        </div>
      </div>

      <div className="mt-5 rounded-lg border border-red-900/50 bg-red-950/20 p-5">
        <div className="text-sm font-medium text-red-300">Danger zone</div>
        <p className="mt-1 text-xs text-neutral-500">Disconnecting removes this repo, its workflows, and run history from actions-toolkit.</p>
        <button
          type="button"
          onClick={() => setConfirmDelete(true)}
          className="mt-3 rounded-md border border-red-800 px-3 py-1.5 text-sm text-red-300 hover:bg-red-950/40"
        >
          Disconnect repo
        </button>
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
