import type { ReactNode } from "react";
import { LogOut } from "lucide-react";
import Sidebar from "./Sidebar";
import ThemeToggle from "../common/ThemeToggle";
import GithubReconnectBanner from "../settings/GithubReconnectBanner";
import Button from "../common/Button";
import { useLogout } from "../../hooks/useAuth";
import type { User } from "../../api/types";

export default function AppShell({ user, children }: { user: User; children: ReactNode }) {
  const logout = useLogout();

  return (
    <div className="flex h-full w-full">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col">
        <header className="flex h-12 shrink-0 items-center justify-end gap-3 border-b border-neutral-800 px-4">
          <ThemeToggle />
          <span className="h-4 w-px bg-neutral-800" />
          <span className="text-sm text-neutral-400">{user.username}</span>
          <Button variant="default" size="sm" onClick={() => logout.mutate()} aria-label="Log out">
            <LogOut className="h-3.5 w-3.5" strokeWidth={2} />
            Log out
          </Button>
        </header>
        <GithubReconnectBanner />
        <main className="min-w-0 flex-1 overflow-y-auto p-6">{children}</main>
      </div>
    </div>
  );
}
