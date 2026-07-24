import { AlertTriangle, CheckCircle2, Loader2 } from "lucide-react";
import Button from "../common/Button";
import type { TunnelState } from "../../api/types";

/**
 * Shared start/status block for a one-click tunnel (Cloudflare or Tailscale) -- used both by a
 * repo's own webhook tunnel (RepoWebhooksPage) and the instance's dashboard tunnel
 * (DashboardTunnelSettingsPage).
 */
export default function TunnelControl({
  status,
  onStart,
  starting,
  installed,
  binaryLabel,
}: {
  status: TunnelState | undefined;
  onStart: () => void;
  starting: boolean;
  installed: boolean | undefined;
  binaryLabel: string;
}) {
  if (installed === false) {
    return (
      <p className="mt-3 flex items-start gap-1.5 rounded-md border border-neutral-800 bg-neutral-950 px-3 py-2 text-xs text-neutral-500">
        <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0 text-[var(--color-status-warning)]" strokeWidth={2} />
        {binaryLabel} isn't installed on this machine, so this instance can't start the tunnel for you. Install it, then reopen this dialog.
      </p>
    );
  }

  if (status?.status === "running") {
    return (
      <div className="mt-3 flex items-center gap-1.5 text-xs text-[var(--color-status-success)]">
        <CheckCircle2 className="h-3.5 w-3.5 shrink-0" strokeWidth={2} />
        Tunnel running
      </div>
    );
  }

  return (
    <div className="mt-3">
      <Button variant="primary" size="sm" onClick={onStart} disabled={starting || status?.status === "starting" || installed === undefined}>
        {status?.status === "starting" && <Loader2 className="h-3.5 w-3.5 animate-spin" strokeWidth={2} />}
        {status?.status === "starting" ? "Starting…" : "Start tunnel"}
      </Button>
      {status?.status === "starting" && (
        <p className="mt-2 text-xs text-neutral-500">Waiting to report a tunnel URL, usually a few seconds…</p>
      )}
      {status?.status === "failed" && (
        <p className="mt-2 flex items-start gap-1.5 text-xs text-[var(--color-status-error)]">
          <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" strokeWidth={2} />
          {status.message}
        </p>
      )}
    </div>
  );
}
