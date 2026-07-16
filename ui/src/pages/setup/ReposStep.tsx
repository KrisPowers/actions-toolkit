import { useMemo, useState } from "react";
import { useAccessibleRepos } from "../../hooks/useGithubAccount";
import { useCreateRepo } from "../../hooks/useRepos";

export default function ReposStep({ onNext }: { onNext: () => void }) {
  const { data: repos, isLoading } = useAccessibleRepos(true);
  const createRepo = useCreateRepo();
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [connecting, setConnecting] = useState(false);
  const [query, setQuery] = useState("");

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return repos ?? [];
    return (repos ?? []).filter((r) => r.full_name.toLowerCase().includes(q));
  }, [repos, query]);

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
    const chosen = (repos ?? []).filter((r) => selected.has(r.full_name));
    for (const repo of chosen) {
      await createRepo.mutateAsync({ owner: repo.owner, name: repo.name, defaultBranch: repo.default_branch }).catch(() => {
        // continue connecting the rest even if one repo (e.g. already connected) fails
      });
    }
    setConnecting(false);
    onNext();
  }

  return (
    <div>
      <h1 className="text-lg font-semibold text-neutral-100">Choose repos to connect</h1>
      <p className="mt-1 text-sm text-neutral-400">
        Pick any repos you want to run workflows for. You can always connect more later from the Repos page.
      </p>

      <input
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        placeholder="Search repos…"
        className="mt-4 w-full rounded-md border border-neutral-700 bg-neutral-950 px-3 py-2 text-sm text-neutral-100 outline-none focus:border-accent"
      />

      <div className="mt-3 max-h-56 overflow-y-auto rounded-md border border-neutral-800">
        {isLoading && <p className="p-3 text-sm text-neutral-500">Loading repos…</p>}
        {!isLoading && filtered.length === 0 && <p className="p-3 text-sm text-neutral-500">No repos found.</p>}
        {filtered.map((r) => (
          <label key={r.full_name} className="flex items-center gap-2 border-b border-neutral-800 px-3 py-2 last:border-b-0 hover:bg-neutral-800/50">
            <input type="checkbox" checked={selected.has(r.full_name)} onChange={() => toggle(r.full_name)} />
            <span className="text-sm text-neutral-200">{r.full_name}</span>
            {r.private && <span className="text-xs text-neutral-600">private</span>}
          </label>
        ))}
      </div>

      <button
        type="button"
        onClick={connectSelected}
        disabled={selected.size === 0 || connecting}
        className="mt-5 w-full rounded-md bg-accent px-3 py-2 text-sm font-medium text-white hover:bg-accent-dark disabled:opacity-60"
      >
        {connecting ? "Connecting…" : `Connect ${selected.size || ""} repo${selected.size === 1 ? "" : "s"}`.trim()}
      </button>
      <button type="button" onClick={onNext} className="mt-2 w-full rounded-md px-3 py-2 text-xs text-neutral-500 hover:text-neutral-300">
        Skip for now
      </button>
    </div>
  );
}
