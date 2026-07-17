import { useMemo, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { CheckCircle2, ChevronDown, ChevronUp, Lock, Search } from "lucide-react";
import { useCreateRepo, useRepos } from "../hooks/useRepos";
import { useAccessibleRepos, useGithubTokenStatus } from "../hooks/useGithubAccount";
import GithubTokenHelp from "../components/settings/GithubTokenHelp";
import GithubMark from "../components/common/GithubMark";
import type { CreateRepoResponse } from "../api/repos";

export default function RepoConnectPage() {
  const { data: tokenStatus } = useGithubTokenStatus();
  const { data: connectedRepos } = useRepos();
  const { data: accessibleRepos, isLoading } = useAccessibleRepos(!!tokenStatus?.connected);
  const create = useCreateRepo();
  const navigate = useNavigate();

  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [connecting, setConnecting] = useState(false);
  const [connected, setConnected] = useState<CreateRepoResponse[]>([]);
  const [showManual, setShowManual] = useState(false);
  const [manualOwner, setManualOwner] = useState("");
  const [manualName, setManualName] = useState("");
  const [manualBranch, setManualBranch] = useState("main");

  const alreadyConnected = useMemo(
    () => new Set((connectedRepos ?? []).map((r) => `${r.owner}/${r.name}`)),
    [connectedRepos],
  );

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return (accessibleRepos ?? [])
      .filter((r) => !alreadyConnected.has(r.full_name))
      .filter((r) => !q || r.full_name.toLowerCase().includes(q));
  }, [accessibleRepos, alreadyConnected, query]);

  function toggle(fullName: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(fullName)) next.delete(fullName);
      else next.add(fullName);
      return next;
    });
  }

  async function connectSelected() {
    setConnecting(true);
    const chosen = filtered.filter((r) => selected.has(r.full_name));
    const results: CreateRepoResponse[] = [];
    for (const repo of chosen) {
      try {
        const res = await create.mutateAsync({ owner: repo.owner, name: repo.name, defaultBranch: repo.default_branch });
        results.push(res);
      } catch {
        // one repo failing (e.g. race with another connect) shouldn't block the rest
      }
    }
    setConnecting(false);
    setConnected(results);
  }

  async function connectManual(e: React.FormEvent) {
    e.preventDefault();
    setConnecting(true);
    try {
      const res = await create.mutateAsync({ owner: manualOwner.trim(), name: manualName.trim(), defaultBranch: manualBranch.trim() || "main" });
      setConnected([res]);
    } finally {
      setConnecting(false);
    }
  }

  if (connected.length > 0) {
    return (
      <div className="max-w-2xl">
        <div className="flex items-center gap-2">
          <CheckCircle2 className="h-5 w-5 text-[var(--color-status-success)]" strokeWidth={2} />
          <h1 className="text-lg font-semibold text-neutral-100">
            {connected.length === 1 ? "Repo connected" : `${connected.length} repos connected`}
          </h1>
        </div>
        <p className="mt-2 text-sm text-neutral-400">
          To trigger workflows from push, pull request, and release events, add a webhook on each repo pointing at the
          URL below. If this machine isn't publicly reachable, tunnel it first (e.g. <code>ngrok http</code>) and use
          the tunnel URL instead.
        </p>

        <div className="mt-4 flex flex-col gap-3">
          {connected.map((repo) => (
            <div key={repo.id} className="rounded-lg border border-neutral-800 bg-neutral-900 p-4">
              <div className="text-sm font-medium text-neutral-200">
                {repo.owner}/{repo.name}
              </div>
              <div className="mt-2 text-xs font-medium text-neutral-500">Webhook payload URL</div>
              <code className="mt-1 block break-all rounded bg-neutral-950 px-2 py-1 text-xs text-neutral-200">{repo.webhook_url}</code>
              <div className="mt-2 text-xs font-medium text-neutral-500">Webhook secret</div>
              <code className="mt-1 block break-all rounded bg-neutral-950 px-2 py-1 text-xs text-neutral-200">{repo.webhook_secret}</code>
            </div>
          ))}
        </div>
        <p className="mt-3 text-xs text-neutral-500">
          In each repo on GitHub: Settings → Webhooks → Add webhook. Set content type to <code>application/json</code>
          and paste the matching secret above.
        </p>

        <button
          type="button"
          onClick={() => navigate("/repos")}
          className="mt-5 rounded-md bg-accent px-3 py-1.5 text-sm font-medium text-white hover:bg-accent-hover"
        >
          Done
        </button>
      </div>
    );
  }

  if (!tokenStatus?.connected) {
    return (
      <div className="max-w-md">
        <div className="flex items-center gap-2">
          <GithubMark className="h-5 w-5 text-neutral-500" />
          <h1 className="text-lg font-semibold text-neutral-100">Connect a repo</h1>
        </div>
        <p className="mt-3 flex items-center gap-1.5 text-sm text-neutral-400">
          You need a GitHub token first.
          <GithubTokenHelp />
        </p>
        <Link to="/settings" className="mt-3 inline-block text-sm text-accent hover:underline">
          Add one in Settings →
        </Link>
      </div>
    );
  }

  return (
    <div className="max-w-lg">
      <h1 className="text-lg font-semibold text-neutral-100">Connect a repo</h1>
      <p className="mt-1 text-sm text-neutral-400">Connected as @{tokenStatus.github_login}. Pick from repos this token can see.</p>

      <div className="relative mt-4">
        <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-neutral-500" strokeWidth={2} />
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search repos…"
          className="w-full rounded-md border border-neutral-700 bg-neutral-950 py-2 pl-9 pr-3 text-sm text-neutral-100 outline-none focus:border-accent"
        />
      </div>

      <div className="mt-3 max-h-72 overflow-y-auto rounded-md border border-neutral-800">
        {isLoading && <p className="p-3 text-sm text-neutral-500">Loading repos…</p>}
        {!isLoading && filtered.length === 0 && <p className="p-3 text-sm text-neutral-500">No repos found.</p>}
        {filtered.map((r) => (
          <label
            key={r.full_name}
            className="flex items-center gap-2 border-b border-neutral-800 px-3 py-2 last:border-b-0 hover:bg-neutral-800/50"
          >
            <input type="checkbox" checked={selected.has(r.full_name)} onChange={() => toggle(r.full_name)} />
            <span className="flex-1 text-sm text-neutral-200">{r.full_name}</span>
            {r.private && <Lock className="h-3.5 w-3.5 text-neutral-600" strokeWidth={2} />}
          </label>
        ))}
      </div>

      <button
        type="button"
        onClick={connectSelected}
        disabled={selected.size === 0 || connecting}
        className="mt-4 w-full rounded-md bg-accent px-3 py-2 text-sm font-medium text-white hover:bg-accent-hover disabled:opacity-60"
      >
        {connecting ? "Connecting…" : `Connect ${selected.size || ""} repo${selected.size === 1 ? "" : "s"}`.trim()}
      </button>

      <button
        type="button"
        onClick={() => setShowManual((v) => !v)}
        className="mt-3 inline-flex items-center gap-1 text-xs text-neutral-500 hover:text-neutral-300"
      >
        {showManual ? <ChevronUp className="h-3.5 w-3.5" strokeWidth={2} /> : <ChevronDown className="h-3.5 w-3.5" strokeWidth={2} />}
        {showManual ? "Hide manual entry" : "Not seeing a repo? Add it by owner/name"}
      </button>

      {showManual && (
        <form onSubmit={connectManual} className="mt-3 rounded-lg border border-neutral-800 bg-neutral-900 p-4">
          <label className="block text-xs font-medium text-neutral-400">Owner</label>
          <input
            value={manualOwner}
            onChange={(e) => setManualOwner(e.target.value)}
            className="mt-1 w-full rounded-md border border-neutral-700 bg-neutral-950 px-3 py-2 text-sm text-neutral-100 outline-none focus:border-accent"
          />
          <label className="mt-3 block text-xs font-medium text-neutral-400">Repository name</label>
          <input
            value={manualName}
            onChange={(e) => setManualName(e.target.value)}
            className="mt-1 w-full rounded-md border border-neutral-700 bg-neutral-950 px-3 py-2 text-sm text-neutral-100 outline-none focus:border-accent"
          />
          <label className="mt-3 block text-xs font-medium text-neutral-400">Default branch</label>
          <input
            value={manualBranch}
            onChange={(e) => setManualBranch(e.target.value)}
            className="mt-1 w-full rounded-md border border-neutral-700 bg-neutral-950 px-3 py-2 text-sm text-neutral-100 outline-none focus:border-accent"
          />
          <button
            type="submit"
            disabled={!manualOwner || !manualName || connecting}
            className="mt-4 w-full rounded-md border border-neutral-700 px-3 py-2 text-sm text-neutral-200 hover:bg-neutral-800 disabled:opacity-50"
          >
            Connect repo
          </button>
        </form>
      )}
    </div>
  );
}
