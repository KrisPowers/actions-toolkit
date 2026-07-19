import { useMemo, useState } from "react";
import { Lock, Search } from "lucide-react";
import { useAccessibleRepos } from "../../hooks/useGithubAccount";
import { useCreateRepo } from "../../hooks/useRepos";
import Avatar from "../../components/common/Avatar";
import Button from "../../components/common/Button";
import Input from "../../components/common/Input";
import Checkbox from "../../components/common/Checkbox";

export default function ReposStep({ onNext }: { onNext: () => void }) {
  const { data: repos, isLoading } = useAccessibleRepos(true);
  const createRepo = useCreateRepo();
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [connecting, setConnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);
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
    setError(null);
    const chosen = (repos ?? []).filter((r) => selected.has(r.full_name));
    const failures: string[] = [];
    for (const repo of chosen) {
      await createRepo.mutateAsync({ owner: repo.owner, name: repo.name, defaultBranch: repo.default_branch }).catch((e) => {
        // continue connecting the rest even if one repo (e.g. already connected) fails, but
        // still surface it instead of silently advancing past a failed connection
        failures.push(`${repo.full_name}: ${e instanceof Error ? e.message : "failed to connect"}`);
      });
    }
    setConnecting(false);
    if (failures.length > 0) {
      setError(failures.join("\n"));
      return;
    }
    onNext();
  }

  return (
    <div>
      <h1 className="text-lg font-semibold text-neutral-100">Choose repos to connect</h1>
      <p className="mt-1 text-sm text-neutral-400">
        Pick any repos you want to run workflows for. You can always connect more later from the Repos page.
      </p>

      <div className="relative mt-4">
        <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-neutral-500" strokeWidth={2} />
        <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search repos…" className="w-full py-2 pl-9 pr-3" />
      </div>

      <div className="mt-3 max-h-56 overflow-y-auto rounded-md border border-neutral-800">
        {isLoading && <p className="p-3 text-sm text-neutral-500">Loading repos…</p>}
        {!isLoading && filtered.length === 0 && <p className="p-3 text-sm text-neutral-500">No repos found.</p>}
        {filtered.map((r) => (
          <label key={r.full_name} className="flex items-center gap-2 border-b border-neutral-800 px-3 py-2 last:border-b-0 hover:bg-neutral-800/50">
            <Checkbox checked={selected.has(r.full_name)} onChange={() => toggle(r.full_name)} />
            <Avatar login={r.owner} size={18} />
            <span className="flex-1 text-sm text-neutral-200">{r.full_name}</span>
            {r.private && <Lock className="h-3.5 w-3.5 text-neutral-600" strokeWidth={2} />}
          </label>
        ))}
      </div>

      <Button variant="primary" onClick={connectSelected} disabled={selected.size === 0 || connecting} className="mt-5 w-full">
        {connecting ? "Connecting…" : `Connect ${selected.size || ""} repo${selected.size === 1 ? "" : "s"}`.trim()}
      </Button>
      {error && <p className="mt-2 whitespace-pre-line text-sm text-[var(--color-status-error)]">{error}</p>}
      <button type="button" onClick={onNext} className="mt-2 w-full rounded-md px-3 py-2 text-xs text-neutral-500 hover:text-neutral-300">
        Skip for now
      </button>
    </div>
  );
}
