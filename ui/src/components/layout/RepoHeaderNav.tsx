import { Link, matchPath, useLocation } from "react-router-dom";
import { ChevronDown, Plus } from "lucide-react";
import {
  AlertTriangle,
  CircleDot,
  GitPullRequest,
  Package,
  PlayCircle,
  ScrollText,
  SlidersHorizontal,
  Tag,
  Workflow,
} from "lucide-react";
import { useRepos } from "../../hooks/useRepos";
import Avatar from "../common/Avatar";
import Menu from "../common/Menu";
import { TabLink } from "../common/Tabs";

const TABS = [
  { path: "workflows", icon: Workflow, label: "Workflows" },
  { path: "runs", icon: PlayCircle, label: "Runs" },
  { path: "logs", icon: ScrollText, label: "Logs" },
  { path: "artifacts", icon: Package, label: "Artifacts" },
  { path: "events", icon: AlertTriangle, label: "Flagged Events" },
  { path: "issues", icon: CircleDot, label: "Issues" },
  { path: "pulls", icon: GitPullRequest, label: "Pull Requests" },
  { path: "releases", icon: Tag, label: "Releases" },
  { path: "settings", icon: SlidersHorizontal, label: "Settings" },
];

// RepoHeaderNav is rendered in AppShell as a sibling of <AppRoutes>'s <Routes>, not as a
// descendant of the matched Route, so useParams() has no route context here and always returns
// {}. matchPath against the raw location works from anywhere, matched-tree or not.
function useRepoIdFromLocation(): string | undefined {
  const location = useLocation();
  const match = matchPath("/repos/:repoId/*", location.pathname);
  return match?.params.repoId;
}

export default function RepoHeaderNav() {
  const repoId = useRepoIdFromLocation();
  const { data: repos } = useRepos();
  const currentRepo = repos?.find((r) => r.id === repoId);

  if (!repoId) return null;

  return (
    <div className="border-b border-neutral-800 bg-neutral-950 px-4">
      <Menu
        align="left"
        trigger={({ toggle, open }) => (
          <button type="button" onClick={toggle} aria-expanded={open} className="flex items-center gap-2 py-3 text-base font-semibold text-neutral-100">
            {currentRepo ? (
              <>
                <Avatar login={currentRepo.owner} size={20} />
                {currentRepo.owner}
                <span className="text-neutral-500">/</span>
                {currentRepo.name}
              </>
            ) : (
              "This repo"
            )}
            <ChevronDown className="h-4 w-4 text-neutral-500" strokeWidth={2} />
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

      <nav className="flex gap-5 overflow-x-auto">
        {TABS.map((tab) => (
          <TabLink key={tab.path} to={`/repos/${repoId}/${tab.path}`} icon={tab.icon}>
            {tab.label}
          </TabLink>
        ))}
      </nav>
    </div>
  );
}
