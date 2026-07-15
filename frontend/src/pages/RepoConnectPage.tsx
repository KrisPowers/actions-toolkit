import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useCreateRepo } from "../hooks/useRepos";

export default function RepoConnectPage() {
  const [owner, setOwner] = useState("");
  const [name, setName] = useState("");
  const [defaultBranch, setDefaultBranch] = useState("main");
  const [pat, setPat] = useState("");
  const [result, setResult] = useState<{ webhookUrl: string; webhookSecret: string } | null>(null);
  const create = useCreateRepo();
  const navigate = useNavigate();

  function submit(e: React.FormEvent) {
    e.preventDefault();
    create.mutate(
      { owner, name, pat, defaultBranch },
      {
        onSuccess: (repo) => setResult({ webhookUrl: repo.webhook_url, webhookSecret: repo.webhook_secret }),
      },
    );
  }

  if (result) {
    return (
      <div className="max-w-xl">
        <h1 className="text-lg font-semibold text-neutral-100">Repo connected</h1>
        <p className="mt-2 text-sm text-neutral-400">
          To receive events (push, pull request, release) without paying for GitHub-hosted runners, add a webhook on the
          GitHub repo pointing at this server. If this machine isn't publicly reachable, tunnel it (e.g. <code>ngrok http</code>)
          and use the tunnel URL instead.
        </p>

        <div className="mt-4 rounded-lg border border-neutral-800 bg-neutral-900 p-4">
          <div className="text-xs font-medium text-neutral-500">Webhook payload URL (append to your public base URL)</div>
          <code className="mt-1 block break-all rounded bg-neutral-950 px-2 py-1 text-xs text-neutral-200">{result.webhookUrl}</code>

          <div className="mt-3 text-xs font-medium text-neutral-500">Webhook secret</div>
          <code className="mt-1 block break-all rounded bg-neutral-950 px-2 py-1 text-xs text-neutral-200">{result.webhookSecret}</code>

          <p className="mt-3 text-xs text-neutral-500">
            In the GitHub repo, go to Settings → Webhooks → Add webhook, set content type to <code>application/json</code>, paste
            the secret above, and select the events you want to trigger workflows for (push, pull requests, releases).
          </p>
        </div>

        <button
          type="button"
          onClick={() => navigate("/repos")}
          className="mt-5 rounded-md bg-accent px-3 py-1.5 text-sm font-medium text-white hover:bg-accent-dark"
        >
          Done
        </button>
      </div>
    );
  }

  return (
    <div className="max-w-md">
      <h1 className="text-lg font-semibold text-neutral-100">Connect a repo</h1>
      <form onSubmit={submit} className="mt-4 rounded-lg border border-neutral-800 bg-neutral-900 p-5">
        <label className="block text-xs font-medium text-neutral-400">Owner</label>
        <input
          value={owner}
          onChange={(e) => setOwner(e.target.value)}
          placeholder="e.g. your GitHub username or org"
          className="mt-1 w-full rounded-md border border-neutral-700 bg-neutral-950 px-3 py-2 text-sm text-neutral-100 outline-none focus:border-accent"
        />

        <label className="mt-4 block text-xs font-medium text-neutral-400">Repository name</label>
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          className="mt-1 w-full rounded-md border border-neutral-700 bg-neutral-950 px-3 py-2 text-sm text-neutral-100 outline-none focus:border-accent"
        />

        <label className="mt-4 block text-xs font-medium text-neutral-400">Default branch</label>
        <input
          value={defaultBranch}
          onChange={(e) => setDefaultBranch(e.target.value)}
          className="mt-1 w-full rounded-md border border-neutral-700 bg-neutral-950 px-3 py-2 text-sm text-neutral-100 outline-none focus:border-accent"
        />

        <label className="mt-4 block text-xs font-medium text-neutral-400">Personal access token</label>
        <input
          type="password"
          value={pat}
          onChange={(e) => setPat(e.target.value)}
          placeholder="ghp_…"
          className="mt-1 w-full rounded-md border border-neutral-700 bg-neutral-950 px-3 py-2 text-sm text-neutral-100 outline-none focus:border-accent"
        />
        <p className="mt-1 text-xs text-neutral-500">
          Needs repo scope (contents, issues, pull requests, releases). Stored encrypted at rest and never shown again.
        </p>

        {create.isError && <p className="mt-3 text-sm text-red-400">{(create.error as Error).message}</p>}

        <button
          type="submit"
          disabled={create.isPending}
          className="mt-5 w-full rounded-md bg-accent px-3 py-2 text-sm font-medium text-white hover:bg-accent-dark disabled:opacity-60"
        >
          {create.isPending ? "Connecting…" : "Connect repo"}
        </button>
      </form>
    </div>
  );
}
