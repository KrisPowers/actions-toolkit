import { useState } from "react";
import { ShieldCheck, Trash2 } from "lucide-react";
import { useDeleteGithubToken, useGithubTokenStatus } from "../../hooks/useGithubAccount";
import ConfirmDialog from "../common/ConfirmDialog";
import GithubConnectButton from "./GithubConnectButton";
import GithubMark from "../common/GithubMark";
import Card from "../common/Card";

export default function GithubConnectionCard() {
  const { data: status } = useGithubTokenStatus();
  const deleteToken = useDeleteGithubToken();
  const [confirmRemove, setConfirmRemove] = useState(false);

  return (
    <Card className="p-5">
      <div className="flex items-center gap-2">
        <GithubMark className="h-4 w-4 text-neutral-500" />
        <h2 className="text-sm font-semibold text-neutral-200">GitHub connection</h2>
      </div>

      {status?.connected ? (
        <>
          <p className="mt-2 flex items-center gap-1.5 text-sm text-neutral-300">
            <ShieldCheck className="h-4 w-4 text-[var(--color-status-success)]" strokeWidth={2} />
            Connected as <span className="font-medium text-neutral-100">@{status.github_login}</span>
          </p>
          <p className="mt-1 text-xs text-neutral-500">
            {status.token_type === "github_app"
              ? "Connected via the actions-toolkit GitHub App."
              : "Connected via a legacy personal access token."}
            {status.needs_reconnect && " Reconnect through the GitHub App below to clear the banner above."}
          </p>

          {status.needs_reconnect && (
            <div className="mt-3">
              <GithubConnectButton label="Reconnect" variant="outline" />
            </div>
          )}
        </>
      ) : (
        <>
          <p className="mt-2 text-sm text-neutral-500">No GitHub connection yet.</p>
          <div className="mt-4">
            <GithubConnectButton />
          </div>
        </>
      )}

      {status?.connected && (
        <div className="mt-4 border-t border-neutral-800 pt-4">
          <button
            type="button"
            onClick={() => setConfirmRemove(true)}
            className="inline-flex items-center gap-1.5 text-xs text-[var(--color-status-error)] hover:underline"
          >
            <Trash2 className="h-3.5 w-3.5" strokeWidth={2} />
            Disconnect
          </button>
        </div>
      )}

      <ConfirmDialog
        open={confirmRemove}
        title="Disconnect GitHub"
        message="Workflow dispatch, webhooks, and issue/PR/release actions stop working until you reconnect."
        confirmLabel="Disconnect"
        danger
        onCancel={() => setConfirmRemove(false)}
        onConfirm={() => {
          deleteToken.mutate();
          setConfirmRemove(false);
        }}
      />
    </Card>
  );
}
