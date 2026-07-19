import { Link } from "react-router-dom";
import { LogOut, Settings as SettingsIcon } from "lucide-react";
import GithubMark from "../common/GithubMark";
import Menu from "../common/Menu";
import Avatar from "../common/Avatar";
import ThemeToggle from "../common/ThemeToggle";
import RepoSwitcher from "./RepoSwitcher";
import { useLogout } from "../../hooks/useAuth";
import type { User } from "../../api/types";

// GitHub's global nav stays this dark shade regardless of the site's light/dark theme setting,
// so this header is styled directly off --color-header-* rather than the theme-flipping neutral
// tokens the rest of the app uses.
const headerIconButton = "text-header-fg-muted hover:bg-white/10 hover:text-header-fg";

export default function GlobalHeader({ user }: { user: User }) {
  const logout = useLogout();

  return (
    <header className="flex h-14 shrink-0 items-center gap-4 border-b border-header-border bg-header-bg px-4 text-header-fg">
      <Link to="/" className="flex items-center rounded-md p-1.5 hover:bg-white/10" aria-label="Dashboard">
        <GithubMark className="h-6 w-6 text-header-fg" />
      </Link>

      <RepoSwitcher />

      <div className="flex-1" />

      <ThemeToggle triggerClassName={headerIconButton} />

      <Menu
        align="right"
        trigger={({ toggle, open }) => (
          <button type="button" onClick={toggle} aria-expanded={open} aria-label="User menu" className="flex items-center gap-1.5 rounded-md p-1 hover:bg-white/10">
            <Avatar login={user.username} size={24} />
          </button>
        )}
      >
        <div className="px-2.5 py-1.5 text-xs text-neutral-500">
          Signed in as <span className="font-semibold text-neutral-200">{user.username}</span>
        </div>
        <div className="my-1 h-px bg-neutral-800" />
        <Link to="/settings" className="flex items-center gap-2 rounded-md px-2.5 py-1.5 text-sm text-neutral-300 hover:bg-neutral-800 hover:text-neutral-100">
          <SettingsIcon className="h-3.5 w-3.5" strokeWidth={2} />
          Settings
        </Link>
        <button
          type="button"
          onClick={() => logout.mutate()}
          className="flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-sm text-neutral-300 hover:bg-neutral-800 hover:text-neutral-100"
        >
          <LogOut className="h-3.5 w-3.5" strokeWidth={2} />
          Log out
        </button>
      </Menu>
    </header>
  );
}
