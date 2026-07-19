import { Link } from "react-router-dom";
import { ChevronDown, Plus } from "lucide-react";
import { useRepoIdFromLocation } from "../../hooks/useRepoIdFromLocation";
import { useRepos } from "../../hooks/useRepos";
import Avatar from "../common/Avatar";
import Menu from "../common/Menu";

export default function RepoSwitcher() {
  const repoId = useRepoIdFromLocation();
  const { data: repos } = useRepos();
  const currentRepo = repos?.find((r) => r.id === repoId);

  if (!repoId) return null;

  return (
    <Menu
      align="left"
      trigger={({ toggle, open }) => (
        <button
          type="button"
          onClick={toggle}
          aria-expanded={open}
          className="flex items-center gap-2 rounded-md px-1.5 py-1.5 text-base font-semibold text-header-fg hover:bg-white/10"
        >
          {currentRepo ? (
            <>
              <Avatar login={currentRepo.owner} size={20} />
              {currentRepo.owner}
              <span className="text-header-fg-muted">/</span>
              {currentRepo.name}
            </>
          ) : (
            "This repo"
          )}
          <ChevronDown className="h-4 w-4 text-header-fg-muted" strokeWidth={2} />
        </button>
      )}
    >
      <div className="px-2.5 py-1.5 text-xs font-semibold uppercase tracking-wide text-neutral-600">Connected repos</div>
      {(repos ?? []).map((r) => (
        <Link
          key={r.id}
          to={`/repos/${r.id}/workflows`}
          className="flex items-center gap-2 rounded-md px-2.5 py-1.5 text-sm text-neutral-300 hover:bg-neutral-800 hover:text-neutral-100"
        >
          <Avatar login={r.owner} size={16} className="shrink-0" />
          <span className="truncate">
            {r.owner}/{r.name}
          </span>
        </Link>
      ))}
      <div className="my-1 h-px bg-neutral-800" />
      <Link to="/repos/connect" className="flex items-center gap-2 rounded-md px-2.5 py-1.5 text-sm text-neutral-300 hover:bg-neutral-800 hover:text-neutral-100">
        <Plus className="h-3.5 w-3.5" strokeWidth={2} />
        Connect a repo
      </Link>
    </Menu>
  );
}
