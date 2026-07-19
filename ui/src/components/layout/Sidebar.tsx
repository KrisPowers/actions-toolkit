import { NavLink, useParams } from "react-router-dom";
import type { LucideIcon } from "lucide-react";
import {
  AlertTriangle,
  CircleDot,
  GitPullRequest,
  LayoutDashboard,
  Package,
  PlayCircle,
  ScrollText,
  Settings,
  SlidersHorizontal,
  Tag,
  Workflow,
} from "lucide-react";
import { useRepos } from "../../hooks/useRepos";
import Avatar from "../common/Avatar";

// A left border on the active item, rather than a fully-rounded filled pill, mirrors GitHub's
// own left-nav idiom (used in github.com's Settings and notification sidebars).
const linkClass = ({ isActive }: { isActive: boolean }) =>
  `flex items-center gap-2 rounded-md border-l-2 py-1.5 pl-2.5 pr-3 text-sm transition-colors ${
    isActive
      ? "border-accent bg-accent/10 font-medium text-neutral-100"
      : "border-transparent text-neutral-400 hover:bg-neutral-800 hover:text-neutral-200"
  }`;

function NavItem({ to, end, icon: Icon, children }: { to: string; end?: boolean; icon: LucideIcon; children: React.ReactNode }) {
  return (
    <NavLink to={to} end={end} className={linkClass}>
      <Icon className="h-4 w-4 shrink-0" strokeWidth={2} />
      <span className="truncate">{children}</span>
    </NavLink>
  );
}

export default function Sidebar() {
  const { data: repos } = useRepos();
  const { repoId } = useParams();
  const currentRepo = repos?.find((r) => r.id === repoId);

  return (
    <aside className="flex w-64 shrink-0 flex-col border-r border-neutral-800 bg-neutral-950 p-3">
      <div className="mb-4 flex items-center gap-2 px-2 py-1">
        <div className="flex h-7 w-7 items-center justify-center rounded-md bg-accent text-sm font-bold text-white">A</div>
        <span className="text-sm font-semibold text-neutral-100">actions-toolkit</span>
      </div>

      <nav className="flex flex-col gap-1">
        <NavItem to="/" end icon={LayoutDashboard}>
          Dashboard
        </NavItem>
      </nav>

      {repoId && (
        <div className="mt-6">
          {currentRepo ? (
            <div className="flex items-center gap-2 px-2">
              <Avatar login={currentRepo.owner} size={16} className="shrink-0" />
              <span className="truncate text-xs font-semibold text-neutral-300">
                {currentRepo.owner}/{currentRepo.name}
              </span>
            </div>
          ) : (
            <div className="px-2 text-xs font-semibold uppercase tracking-wide text-neutral-600">This repo</div>
          )}
          <nav className="mt-2 flex flex-col gap-1">
            <NavItem to={`/repos/${repoId}/workflows`} icon={Workflow}>
              Workflows
            </NavItem>
            <NavItem to={`/repos/${repoId}/runs`} icon={PlayCircle}>
              Runs
            </NavItem>
            <NavItem to={`/repos/${repoId}/logs`} icon={ScrollText}>
              Logs
            </NavItem>
            <NavItem to={`/repos/${repoId}/artifacts`} icon={Package}>
              Artifacts
            </NavItem>
            <NavItem to={`/repos/${repoId}/events`} icon={AlertTriangle}>
              Flagged Events
            </NavItem>
            <NavItem to={`/repos/${repoId}/issues`} icon={CircleDot}>
              Issues
            </NavItem>
            <NavItem to={`/repos/${repoId}/pulls`} icon={GitPullRequest}>
              Pull Requests
            </NavItem>
            <NavItem to={`/repos/${repoId}/releases`} icon={Tag}>
              Releases
            </NavItem>
            <NavItem to={`/repos/${repoId}/settings`} icon={SlidersHorizontal}>
              Repo Settings
            </NavItem>
          </nav>
        </div>
      )}

      <div className="mt-6 min-h-0 flex-1 overflow-y-auto">
        <div className="px-2 text-xs font-semibold uppercase tracking-wide text-neutral-600">Connected repos</div>
        <nav className="mt-2 flex flex-col gap-1">
          {(repos ?? []).map((r) => (
            <NavLink key={r.id} to={`/repos/${r.id}/workflows`} className={linkClass}>
              <Avatar login={r.owner} size={16} className="shrink-0" />
              <span className="truncate">
                {r.owner}/{r.name}
              </span>
            </NavLink>
          ))}
        </nav>
      </div>

      <nav className="mt-3 flex flex-col gap-1 border-t border-neutral-800 pt-3">
        <NavItem to="/settings" icon={Settings}>
          Settings
        </NavItem>
      </nav>
    </aside>
  );
}
