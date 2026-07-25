import { useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { ChevronRight, Plus } from "lucide-react";
import { useRepos } from "../../hooks/useRepos";
import Avatar from "../common/Avatar";
import Input from "../common/Input";
import { buttonClass } from "../common/Button";

const COLLAPSED_COUNT = 6;

export default function DashboardSidebar() {
  const { data: repos } = useRepos();
  const [query, setQuery] = useState("");
  const [expanded, setExpanded] = useState(false);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    const all = repos ?? [];
    if (!q) return all;
    return all.filter((r) => `${r.owner}/${r.name}`.toLowerCase().includes(q));
  }, [repos, query]);

  const visible = expanded ? filtered : filtered.slice(0, COLLAPSED_COUNT);
  const hasMore = filtered.length > COLLAPSED_COUNT;

  return (
    <aside className="flex flex-col gap-3">
      <div className="flex items-center justify-between gap-2">
        <h2 className="text-sm font-semibold text-neutral-200">Top repositories</h2>
        <Link to="/repos/connect" className={buttonClass("primary", "sm")}>
          <Plus className="h-3.5 w-3.5" strokeWidth={2} />
          New
        </Link>
      </div>

      <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Find a repository…" className="w-full" />

      <div className="flex flex-col">
        {visible.map((r) => (
          <Link
            key={r.id}
            to={`/repos/${r.id}/overview`}
            className="flex items-center gap-2 rounded-md px-2 py-1.5 text-sm text-neutral-300 hover:bg-neutral-800/60 hover:text-neutral-100"
          >
            <Avatar login={r.owner} size={18} className="shrink-0" />
            <span className="truncate">
              {r.owner}/{r.name}
            </span>
          </Link>
        ))}
        {filtered.length === 0 && <p className="px-2 py-1.5 text-xs text-neutral-500">No repositories match.</p>}
      </div>

      {hasMore && !expanded && (
        <button
          type="button"
          onClick={() => setExpanded(true)}
          className="flex items-center gap-1 px-2 text-xs font-medium text-neutral-500 hover:text-neutral-300"
        >
          Show more
          <ChevronRight className="h-3 w-3" strokeWidth={2} />
        </button>
      )}
    </aside>
  );
}
