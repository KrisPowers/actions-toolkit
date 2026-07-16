import { useState } from "react";
import { useDeleteGithubToken, useGithubTokenStatus, useSetGithubToken } from "../../hooks/useGithubAccount";
import ConfirmDialog from "../common/ConfirmDialog";

export default function GithubConnectionCard() {
  const { data: status } = useGithubTokenStatus();
  const setToken = useSetGithubToken();
  const deleteToken = useDeleteGithubToken();
  const [newToken, setNewToken] = useState("");
  const [confirmRemove, setConfirmRemove] = useState(false);

  return (
    <div className="rounded-lg border border-neutral-800 bg-neutral-900 p-5">
      <h2 className="text-sm font-semibold text-neutral-200">GitHub connection</h2>

      {status?.connected ? (
        <p className="mt-2 text-sm text-neutral-300">
          Connected as <span className="font-medium text-neutral-100">@{status.github_login}</span>
        </p>
      ) : (
        <p className="mt-2 text-sm text-neutral-500">No GitHub token configured yet.</p>
      )}

      <div className="mt-4">
        <div className="text-xs font-medium text-neutral-400">{status?.connected ? "Rotate token" : "Add a token"}</div>
        <div className="mt-2 flex gap-2">
          <input
            type="password"
            value={newToken}
            onChange={(e) => setNewToken(e.target.value)}
            placeholder="ghp_…"
            className="flex-1 rounded-md border border-neutral-700 bg-neutral-950 px-2.5 py-1.5 text-sm text-neutral-100"
          />
          <button
            type="button"
            disabled={!newToken || setToken.isPending}
            onClick={() => setToken.mutate(newToken, { onSuccess: () => setNewToken("") })}
            className="rounded-md bg-accent px-3 py-1.5 text-sm font-medium text-white hover:bg-accent-dark disabled:opacity-50"
          >
            {setToken.isPending ? "Verifying…" : "Save"}
          </button>
        </div>
        {setToken.isError && <p className="mt-2 text-xs text-red-400">{(setToken.error as Error).message}</p>}
      </div>

      {status?.connected && (
        <div className="mt-4 border-t border-neutral-800 pt-4">
          <button type="button" onClick={() => setConfirmRemove(true)} className="text-xs text-red-400 hover:underline">
            Remove token
          </button>
          <p className="mt-1 text-xs text-neutral-600">
            Connected repos stay listed, but workflow dispatch, webhooks, and issue/PR/release actions stop working
            until a token is set again.
          </p>
        </div>
      )}

      <ConfirmDialog
        open={confirmRemove}
        title="Remove GitHub token"
        message="Workflow dispatch, webhook processing, and GitHub actions (issues/PRs/releases) will stop working until a new token is added."
        confirmLabel="Remove"
        danger
        onCancel={() => setConfirmRemove(false)}
        onConfirm={() => {
          deleteToken.mutate();
          setConfirmRemove(false);
        }}
      />
    </div>
  );
}
