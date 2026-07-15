import { Link } from "react-router-dom";
import { useRepos } from "../hooks/useRepos";

export default function RepoListPage() {
  const { data: repos, isLoading } = useRepos();

  return (
    <div>
      <div className="flex items-center justify-between">
        <h1 className="text-lg font-semibold text-neutral-100">Repositories</h1>
        <Link to="/repos/connect" className="rounded-md bg-accent px-3 py-1.5 text-sm font-medium text-white hover:bg-accent-dark">
          Connect a repo
        </Link>
      </div>

      {isLoading && <p className="mt-6 text-sm text-neutral-500">Loading…</p>}

      {!isLoading && (repos ?? []).length === 0 && (
        <p className="mt-6 text-sm text-neutral-500">No repos connected yet. Connect one to start running workflows locally.</p>
      )}

      <div className="mt-6 grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {(repos ?? []).map((r) => (
          <Link
            key={r.id}
            to={`/repos/${r.id}/workflows`}
            className="rounded-lg border border-neutral-800 bg-neutral-900 p-4 hover:border-accent/50"
          >
            <div className="text-sm font-semibold text-neutral-100">
              {r.owner}/{r.name}
            </div>
            <div className="mt-1 text-xs text-neutral-500">default branch: {r.default_branch}</div>
            <div className="mt-3 text-xs text-neutral-600">PAT {r.pat_masked}</div>
          </Link>
        ))}
      </div>
    </div>
  );
}
