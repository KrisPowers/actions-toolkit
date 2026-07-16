import { NavLink, useParams } from "react-router-dom";
import { useRepos } from "../../hooks/useRepos";

const linkClass = ({ isActive }: { isActive: boolean }) =>
  `block rounded-md px-3 py-1.5 text-sm transition-colors ${
    isActive ? "bg-accent/15 text-accent" : "text-neutral-400 hover:bg-neutral-800 hover:text-neutral-200"
  }`;

export default function Sidebar() {
  const { data: repos } = useRepos();
  const { repoId } = useParams();

  return (
    <aside className="flex w-60 shrink-0 flex-col border-r border-neutral-800 bg-neutral-950 p-3">
      <div className="mb-4 flex items-center gap-2 px-2 py-1">
        <div className="flex h-7 w-7 items-center justify-center rounded-md bg-accent text-sm font-bold text-white">A</div>
        <span className="text-sm font-semibold text-neutral-100">actions-toolkit</span>
      </div>

      <nav className="flex flex-col gap-1">
        <NavLink to="/" end className={linkClass}>
          Dashboard
        </NavLink>
        <NavLink to="/repos" className={linkClass}>
          Repositories
        </NavLink>
        <NavLink to="/settings" className={linkClass}>
          Settings
        </NavLink>
      </nav>

      {repoId && (
        <div className="mt-6">
          <div className="px-2 text-xs font-semibold uppercase tracking-wide text-neutral-600">This repo</div>
          <nav className="mt-2 flex flex-col gap-1">
            <NavLink to={`/repos/${repoId}/workflows`} className={linkClass}>
              Workflows
            </NavLink>
            <NavLink to={`/repos/${repoId}/runs`} className={linkClass}>
              Runs
            </NavLink>
            <NavLink to={`/repos/${repoId}/issues`} className={linkClass}>
              Issues
            </NavLink>
            <NavLink to={`/repos/${repoId}/pulls`} className={linkClass}>
              Pull Requests
            </NavLink>
            <NavLink to={`/repos/${repoId}/releases`} className={linkClass}>
              Releases
            </NavLink>
            <NavLink to={`/repos/${repoId}/settings`} className={linkClass}>
              Repo Settings
            </NavLink>
          </nav>
        </div>
      )}

      <div className="mt-6">
        <div className="px-2 text-xs font-semibold uppercase tracking-wide text-neutral-600">Connected repos</div>
        <nav className="mt-2 flex flex-col gap-1">
          {(repos ?? []).map((r) => (
            <NavLink key={r.id} to={`/repos/${r.id}/workflows`} className={linkClass}>
              {r.owner}/{r.name}
            </NavLink>
          ))}
        </nav>
      </div>
    </aside>
  );
}
