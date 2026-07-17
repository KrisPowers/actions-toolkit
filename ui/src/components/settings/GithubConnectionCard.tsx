import { useState } from "react";
import { ShieldCheck, Trash2 } from "lucide-react";
import { useDeleteGithubToken, useGithubTokenStatus, useSetGithubToken } from "../../hooks/useGithubAccount";
import ConfirmDialog from "../common/ConfirmDialog";
import GithubTokenHelp from "./GithubTokenHelp";
import GithubMark from "../common/GithubMark";

export default function GithubConnectionCard() {
  const { data: status } = useGithubTokenStatus();
  const setToken = useSetGithubToken();
  const deleteToken = useDeleteGithubToken();
  const [newToken, setNewToken] = useState("");
  const [confirmRemove, setConfirmRemove] = useState(false);

  return (
    <div className="rounded-lg border border-neutral-800 bg-neutral-900 p-5">
      <div className="flex items-center gap-2">
        <GithubMark className="h-4 w-4 text-neutral-500" />
        <h2 className="text-sm font-semibold text-neutral-200">GitHub connection</h2>
        <GithubTokenHelp />
      </div>

      {status?.connected ? (
        <p className="mt-2 flex items-center gap-1.5 text-sm text-neutral-300">
          <ShieldCheck className="h-4 w-4 text-[var(--color-status-success)]" strokeWidth={2} />
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
            className="flex-1 rounded-md border border-neutral-700 bg-neutral-950 px-2.5 py-1.5 text-sm text-neutral-100 outline-none focus:border-accent"
          />
          <button
            type="button"
            disabled={!newToken || setToken.isPending}
            onClick={() => setToken.mutate(newToken, { onSuccess: () => setNewToken("") })}
            className="rounded-md bg-accent px-3 py-1.5 text-sm font-medium text-white hover:bg-accent-hover disabled:opacity-50"
          >
            {setToken.isPending ? "Verifying…" : "Save"}
          </button>
        </div>
        {setToken.isError && <p className="mt-2 text-xs text-[var(--color-status-error)]">{(setToken.error as Error).message}</p>}
      </div>

      {status?.connected && (
        <div className="mt-4 border-t border-neutral-800 pt-4">
          <button
            type="button"
            onClick={() => setConfirmRemove(true)}
            className="inline-flex items-center gap-1.5 text-xs text-[var(--color-status-error)] hover:underline"
          >
            <Trash2 className="h-3.5 w-3.5" strokeWidth={2} />
            Remove token
          </button>
        </div>
      )}

      <ConfirmDialog
        open={confirmRemove}
        title="Remove GitHub token"
        message="Workflow dispatch, webhooks, and issue/PR/release actions stop working until a new token is added."
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
