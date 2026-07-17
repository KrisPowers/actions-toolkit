import { useState } from "react";
import { useSetGithubToken } from "../../hooks/useGithubAccount";
import GithubTokenHelp from "../../components/settings/GithubTokenHelp";
import GithubMark from "../../components/common/GithubMark";

export default function TokenStep({ onNext, onSkip }: { onNext: () => void; onSkip: () => void }) {
  const [token, setToken] = useState("");
  const setTokenMutation = useSetGithubToken();

  function submit(e: React.FormEvent) {
    e.preventDefault();
    setTokenMutation.mutate(token, { onSuccess: onNext });
  }

  return (
    <form onSubmit={submit}>
      <div className="flex items-center gap-2">
        <GithubMark className="h-5 w-5 text-neutral-400" />
        <h1 className="text-lg font-semibold text-neutral-100">Connect GitHub</h1>
      </div>
      <p className="mt-1 text-sm text-neutral-400">
        One token, used for every repo you connect: reading webhook events, checking out code, and managing issues,
        PRs, and releases.
      </p>

      <div className="mt-5 flex items-center gap-1.5">
        <label className="block text-xs font-medium text-neutral-400">Personal access token</label>
        <GithubTokenHelp />
      </div>
      <input
        type="password"
        value={token}
        onChange={(e) => setToken(e.target.value)}
        placeholder="ghp_…"
        className="mt-1 w-full rounded-md border border-neutral-700 bg-neutral-950 px-3 py-2 text-sm text-neutral-100 outline-none focus:border-accent"
        autoFocus
      />
      <p className="mt-1 text-xs text-neutral-600">Needs repo scope. Stored encrypted at rest and never shown again.</p>

      {setTokenMutation.isError && <p className="mt-3 text-sm text-[var(--color-status-error)]">{(setTokenMutation.error as Error).message}</p>}

      <button
        type="submit"
        disabled={!token || setTokenMutation.isPending}
        className="mt-5 w-full rounded-md bg-accent px-3 py-2 text-sm font-medium text-white hover:bg-accent-hover disabled:opacity-60"
      >
        {setTokenMutation.isPending ? "Verifying…" : "Verify and continue"}
      </button>
      <button type="button" onClick={onSkip} className="mt-2 w-full rounded-md px-3 py-2 text-xs text-neutral-500 hover:text-neutral-300">
        Skip for now, add this later in Settings
      </button>
    </form>
  );
}
