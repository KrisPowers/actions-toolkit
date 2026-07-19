import { useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { CheckCircle2, ChevronDown, ChevronUp, ExternalLink, LayoutGrid, Lock, Search } from "lucide-react";
import { useCreateRepo, useRepos } from "../hooks/useRepos";
import { useAccessibleRepos, useGithubTokenStatus } from "../hooks/useGithubAccount";
import GithubConnectButton from "../components/settings/GithubConnectButton";
import GithubMark from "../components/common/GithubMark";
import Avatar from "../components/common/Avatar";
import type { CreateRepoResponse } from "../api/repos";

export default function RepoConnectPage() {
  const { data: tokenStatus } = useGithubTokenStatus();
  const { data: connectedRepos } = useRepos();
  const { data: accessibleRepos, isLoading } = useAccessibleRepos(!!tokenStatus?.connected);
  const create = useCreateRepo();
  const navigate = useNavigate();

  const [query, setQuery] = useState("");
  const [selectedOrg, setSelectedOrg] = useState<string | null>(null);
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

  const connectable = useMemo(
    () => (accessibleRepos ?? []).filter((r) => !alreadyConnected.has(r.full_name)),
    [accessibleRepos, alreadyConnected],
  );

  const orgs = useMemo(() => {
    const counts = new Map<string, number>();
    for (const r of connectable) counts.set(r.owner, (counts.get(r.owner) ?? 0) + 1);
    const list = Array.from(counts.entries()).map(([owner, count]) => ({ owner, count }));
    const myLogin = tokenStatus?.github_login;
    list.sort((a, b) => {
      if (a.owner === myLogin) return -1;
      if (b.owner === myLogin) return 1;
      return a.owner.localeCompare(b.owner);
    });
    return list;
  }, [connectable, tokenStatus]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return connectable
      .filter((r) => !selectedOrg || r.owner === selectedOrg)
      .filter((r) => !q || r.full_name.toLowerCase().includes(q));
  }, [connectable, selectedOrg, query]);

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
    const chosen = connectable.filter((r) => selected.has(r.full_name));
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
          A webhook was created automatically on each repo below, so push, pull request, and release events reach
          this instance with no setup on your part.
        </p>

        <div className="mt-4 flex flex-col gap-3">
          {connected.map((repo) => (
            <div key={repo.id} className="rounded-lg border border-neutral-800 bg-neutral-900 p-4">
              <div className="text-sm font-medium text-neutral-200">
                {repo.owner}/{repo.name}
              </div>
            </div>
          ))}
        </div>

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
        <p className="mt-3 text-sm text-neutral-400">Connect your GitHub account first to pick repos to run workflows for.</p>
        <div className="mt-4">
          <GithubConnectButton />
        </div>
      </div>
    );
  }

  return (
    <div className="max-w-lg">
      <h1 className="text-lg font-semibold text-neutral-100">Connect a repo</h1>
      <p className="mt-1 text-sm text-neutral-400">
        Connected as @{tokenStatus.github_login}. Pick from repos the actions-toolkit GitHub App can access.
      </p>
      {tokenStatus.token_type === "github_app" && (
        <a
          href="https://github.com/settings/installations"
          target="_blank"
          rel="noreferrer"
          className="mt-1.5 inline-flex items-center gap-1 text-xs text-accent hover:underline"
        >
          Not seeing a repo? Manage which repos the App can access on GitHub
          <ExternalLink className="h-3 w-3" strokeWidth={2} />
        </a>
      )}

      <div className="mt-4 flex gap-1.5 overflow-x-auto pb-1">
        <button
          type="button"
          onClick={() => setSelectedOrg(null)}
          className={`flex shrink-0 items-center gap-1.5 rounded-full border px-3 py-1.5 text-xs font-medium transition-colors ${
            selectedOrg === null
              ? "border-accent bg-accent/15 text-accent"
              : "border-neutral-700 text-neutral-300 hover:bg-neutral-800"
          }`}
        >
          <LayoutGrid className="h-3.5 w-3.5" strokeWidth={2} />
          All
        </button>
        {orgs.map(({ owner, count }) => (
          <button
            key={owner}
            type="button"
            onClick={() => setSelectedOrg(owner)}
            className={`flex shrink-0 items-center gap-1.5 rounded-full border px-3 py-1.5 text-xs font-medium transition-colors ${
              selectedOrg === owner
                ? "border-accent bg-accent/15 text-accent"
                : "border-neutral-700 text-neutral-300 hover:bg-neutral-800"
            }`}
          >
            <Avatar login={owner} size={16} />
            {owner}
            <span className="text-neutral-500">{count}</span>
          </button>
        ))}
      </div>

      <div className="relative mt-3">
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
            <Avatar login={r.owner} size={20} />
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
