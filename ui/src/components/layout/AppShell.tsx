import type { ReactNode } from "react";
import GlobalHeader from "./GlobalHeader";
import RepoHeaderNav from "./RepoHeaderNav";
import GithubReconnectBanner from "../settings/GithubReconnectBanner";
import type { User } from "../../api/types";

export default function AppShell({ user, children }: { user: User; children: ReactNode }) {
  return (
    <div className="flex h-full w-full flex-col">
      <GlobalHeader user={user} />
      <RepoHeaderNav />
      <GithubReconnectBanner />
      <main className="min-h-0 flex-1 overflow-y-auto p-6">{children}</main>
    </div>
  );
}
