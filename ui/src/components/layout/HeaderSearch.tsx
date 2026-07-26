import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Search, Workflow } from "lucide-react";
import { useQueries } from "@tanstack/react-query";
import { useRepos } from "../../hooks/useRepos";
import { workflowsApi } from "../../api/workflows";
import Avatar from "../common/Avatar";

// Workflow queries are only enabled once the search is open, since fetching every connected
// repo's workflow list on every header render would be wasteful for repos the user never searches.
export default function HeaderSearch() {
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const navigate = useNavigate();
  const { data: repos } = useRepos();

  const workflowResults = useQueries({
    queries: (repos ?? []).map((repo) => ({
      queryKey: ["workflows", "repo", repo.id],
      queryFn: () => workflowsApi.listForRepo(repo.id),
      enabled: open,
      staleTime: 60_000,
    })),
  });

  useEffect(() => {
    if (!open) return;
    function onPointerDown(e: PointerEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    }
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") setOpen(false);
    }
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  // GitHub-style global "/" shortcut: jumps into the search box from anywhere on the page,
  // unless the user is already typing somewhere else.
  useEffect(() => {
    function onSlash(e: KeyboardEvent) {
      if (e.key !== "/" || e.metaKey || e.ctrlKey || e.altKey) return;
      const target = e.target as HTMLElement;
      if (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable) return;
      e.preventDefault();
      inputRef.current?.focus();
      setOpen(true);
    }
    document.addEventListener("keydown", onSlash);
    return () => document.removeEventListener("keydown", onSlash);
  }, []);

  const q = query.trim().toLowerCase();

  const matchedRepos = useMemo(() => {
    if (!q) return [];
    return (repos ?? []).filter((r) => `${r.owner}/${r.name}`.toLowerCase().includes(q)).slice(0, 5);
  }, [repos, q]);

  const matchedWorkflows = !q || !repos
    ? []
    : workflowResults
        .flatMap((res, i) => (res.data ?? []).map((workflow) => ({ workflow, repo: repos[i] })))
        .filter(({ workflow }) => workflow.name.toLowerCase().includes(q))
        .slice(0, 8);

  const showResults = open && q.length > 0;

  function goToRepo(repoId: string) {
    navigate(`/repos/${repoId}/overview`);
    setOpen(false);
    setQuery("");
  }

  function goToWorkflow(repoId: string, workflowId: string) {
    navigate(`/repos/${repoId}/workflows/${workflowId}`);
    setOpen(false);
    setQuery("");
  }

  return (
    <div className="relative w-full max-w-md" ref={ref}>
      <div className="flex h-8 items-center gap-2 rounded-md border border-header-border bg-white/5 px-2.5 text-sm text-header-fg-muted focus-within:border-accent focus-within:bg-white/10">
        <Search className="h-3.5 w-3.5 shrink-0" strokeWidth={2} />
        <div className="relative h-full flex-1">
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onFocus={() => setOpen(true)}
            aria-label="Search repos and workflows"
            className="h-full w-full bg-transparent text-header-fg outline-none"
          />
          {!open && !query && (
            <div className="pointer-events-none absolute inset-0 flex items-center gap-1.5 text-header-fg-muted">
              <span>Type</span>
              <kbd className="flex h-[18px] min-w-[18px] items-center justify-center rounded border border-header-border px-1 font-sans text-xs leading-none text-header-fg-muted">
                /
              </kbd>
              <span>to search</span>
            </div>
          )}
        </div>
      </div>

      {showResults && (
        <div className="absolute top-full z-40 mt-2 max-h-96 w-full overflow-auto rounded-md border border-neutral-800 bg-neutral-900 p-1 shadow-lg">
          {matchedRepos.length === 0 && matchedWorkflows.length === 0 && (
            <div className="px-2.5 py-3 text-center text-xs text-neutral-500">No matches</div>
          )}

          {matchedRepos.length > 0 && (
            <>
              <div className="px-2.5 py-1.5 text-xs font-semibold uppercase tracking-wide text-neutral-600">Repositories</div>
              {matchedRepos.map((r) => (
                <button
                  key={r.id}
                  type="button"
                  onClick={() => goToRepo(r.id)}
                  className="flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-sm text-neutral-300 hover:bg-neutral-800 hover:text-neutral-100"
                >
                  <Avatar login={r.owner} size={16} className="shrink-0" />
                  <span className="truncate">
                    {r.owner}/{r.name}
                  </span>
                </button>
              ))}
            </>
          )}

          {matchedWorkflows.length > 0 && (
            <>
              <div className="px-2.5 py-1.5 text-xs font-semibold uppercase tracking-wide text-neutral-600">Workflows</div>
              {matchedWorkflows.map(({ workflow, repo }) => (
                <button
                  key={workflow.id}
                  type="button"
                  onClick={() => goToWorkflow(workflow.repo_id, workflow.id)}
                  className="flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-sm text-neutral-300 hover:bg-neutral-800 hover:text-neutral-100"
                >
                  <Workflow className="h-3.5 w-3.5 shrink-0 text-neutral-500" strokeWidth={2} />
                  <span className="truncate">{workflow.name}</span>
                  {repo && <span className="ml-auto shrink-0 text-xs text-neutral-600">{repo.owner}/{repo.name}</span>}
                </button>
              ))}
            </>
          )}
        </div>
      )}
    </div>
  );
}
