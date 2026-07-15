import { useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { useDeleteRepo, useRepo, useTestRepoConnection, useUpdateRepoPat } from "../hooks/useRepos";
import ConfirmDialog from "../components/common/ConfirmDialog";

export default function RepoSettingsPage() {
  const { repoId } = useParams();
  const { data: repo } = useRepo(repoId);
  const updatePat = useUpdateRepoPat(repoId as string);
  const testConnection = useTestRepoConnection();
  const deleteRepo = useDeleteRepo();
  const navigate = useNavigate();
  const [pat, setPat] = useState("");
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

        <div className="mt-4 text-sm font-medium text-neutral-200">Personal access token</div>
        <div className="mt-1 text-xs text-neutral-500">Current: {repo.pat_masked}</div>
        <input
          type="password"
          value={pat}
          onChange={(e) => setPat(e.target.value)}
          placeholder="Enter a new token to rotate it"
          className="mt-2 w-full rounded-md border border-neutral-700 bg-neutral-950 px-3 py-2 text-sm text-neutral-100 outline-none focus:border-accent"
        />
        <button
          type="button"
          disabled={!pat || updatePat.isPending}
          onClick={() => updatePat.mutate(pat, { onSuccess: () => setPat("") })}
          className="mt-2 rounded-md border border-neutral-700 px-3 py-1.5 text-sm text-neutral-200 hover:bg-neutral-800 disabled:opacity-50"
        >
          {updatePat.isPending ? "Updating…" : "Update token"}
        </button>

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
