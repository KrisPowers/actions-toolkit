import { ShieldAlert } from "lucide-react";
import { useGithubTokenStatus } from "../../hooks/useGithubAccount";
import GithubConnectButton from "./GithubConnectButton";

/**
 * Site-wide, non-blocking: shown on every page (not just Settings) so a reconnect need doesn't
 * go unnoticed until something else fails first, but never prevents using the rest of the app.
 */
export default function GithubReconnectBanner() {
  const { data: status } = useGithubTokenStatus();
  if (!status?.needs_reconnect) return null;

  return (
    <div className="flex shrink-0 items-center gap-3 border-b border-[var(--color-status-warning)]/40 bg-[var(--color-status-warning)]/10 px-4 py-2 text-xs text-[var(--color-status-warning)]">
      <ShieldAlert className="h-3.5 w-3.5 shrink-0" strokeWidth={2} />
      <span className="flex-1">
        Your GitHub connection needs to be reconnected
        {status.token_type === "pat" ? ", it's still using the old personal access token" : ""}.
      </span>
      <GithubConnectButton label="Reconnect" variant="outline" presentation="modal" />
    </div>
  );
}
