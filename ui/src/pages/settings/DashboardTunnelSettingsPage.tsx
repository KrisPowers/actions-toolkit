import { AlertTriangle, Cloud, Network, Wifi } from "lucide-react";
import {
  useDashboardTunnelRequests,
  useDashboardTunnelStatus,
  useStartDashboardTunnel,
  useTunnelAvailability,
} from "../../hooks/useSettings";
import Card from "../../components/common/Card";
import InfoTooltip from "../../components/common/InfoTooltip";
import TunnelControl from "../../components/webhooks/TunnelControl";

/**
 * Exposes this instance's dashboard/API remotely -- deliberately a separate page (and a
 * separate tunnel, listener, and process on the backend) from a repo's Webhooks page: this
 * tunnel is never shared with any repo's webhook tunnel, and it's reachable by any approved
 * account rather than just receiving signed GitHub payloads, so it carries its own much heavier
 * rate limiting and request auditing (see the log below).
 */
export default function DashboardTunnelSettingsPage() {
  const { data: status } = useDashboardTunnelStatus();
  const { data: tunnelAvailability } = useTunnelAvailability();
  const startTunnel = useStartDashboardTunnel();
  const { data: requests } = useDashboardTunnelRequests();

  const url = status?.status === "running" ? status.url : undefined;

  return (
    <div className="flex flex-col gap-5">
      <Card className="p-5">
        <div className="flex items-center gap-2">
          <Wifi className="h-4 w-4 text-neutral-500" strokeWidth={2} />
          <h2 className="text-sm font-semibold text-neutral-200">Remote access</h2>
        </div>
        <p className="mt-1 text-xs text-neutral-500">
          Expose this instance's dashboard so you can view runs and insights away from your local network.
        </p>

        <p className="mt-3 flex items-start gap-1.5 rounded-md border border-neutral-800 bg-neutral-950 px-3 py-2 text-xs text-neutral-500">
          <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0 text-[var(--color-status-warning)]" strokeWidth={2} />
          This is separate from every repo's own webhook tunnel (set up on that repo's Webhooks page) -- it never shares a
          process or port with any of them. Anyone with an approved account on this instance can sign in through it, so it's
          protected by much stricter rate limiting and every request is logged below.
        </p>

        {url && (
          <div className="mt-3 flex items-center gap-1 text-xs text-neutral-600">
            Dashboard URL: <code className="text-neutral-400">{url}</code>
            <InfoTooltip text="Anyone who has this URL can reach the login page; only approved accounts can sign in past it." />
          </div>
        )}

        <div className="mt-4 grid grid-cols-1 gap-4 sm:grid-cols-2">
          <div className="rounded-md border border-neutral-800 bg-neutral-900 p-4">
            <div className="flex items-center gap-2">
              <Cloud className="h-5 w-5 text-neutral-400" strokeWidth={2} />
              <div className="text-sm font-medium text-neutral-200">Cloudflare Tunnel</div>
            </div>
            <p className="mt-1 text-xs text-neutral-500">One click, no router access needed.</p>
            <TunnelControl
              status={status}
              onStart={() => startTunnel.mutate("cloudflare")}
              starting={startTunnel.isPending}
              installed={tunnelAvailability?.cloudflared_available}
              binaryLabel="cloudflared"
            />
          </div>

          <div className="rounded-md border border-neutral-800 bg-neutral-900 p-4">
            <div className="flex items-center gap-2">
              <Network className="h-5 w-5 text-neutral-400" strokeWidth={2} />
              <div className="text-sm font-medium text-neutral-200">Tailscale Funnel</div>
            </div>
            <p className="mt-1 text-xs text-neutral-500">One click, exposes this instance over your tailnet.</p>
            <TunnelControl
              status={status}
              onStart={() => startTunnel.mutate("tailscale")}
              starting={startTunnel.isPending}
              installed={tunnelAvailability?.tailscale_available}
              binaryLabel="tailscale"
            />
          </div>
        </div>
      </Card>

      <Card className="p-5">
        <h3 className="text-sm font-semibold text-neutral-200">Recent tunnel requests</h3>
        <p className="mt-1 text-xs text-neutral-500">Every request that arrived over this tunnel, including rate-limited ones.</p>

        <div className="mt-4 overflow-x-auto">
          <table className="w-full text-left text-sm">
            <thead>
              <tr className="border-b border-neutral-800 text-xs text-neutral-500">
                <th className="py-2 pr-4 font-medium">IP address</th>
                <th className="py-2 pr-4 font-medium">Method</th>
                <th className="py-2 pr-4 font-medium">Path</th>
                <th className="py-2 pr-4 font-medium">Status</th>
                <th className="py-2 pr-4 font-medium">Time</th>
              </tr>
            </thead>
            <tbody>
              {(requests ?? []).map((r) => (
                <tr key={r.id} className="border-b border-neutral-900">
                  <td className="py-2 pr-4 text-neutral-400">{r.ip_address ?? "—"}</td>
                  <td className="py-2 pr-4 text-neutral-400">{r.method}</td>
                  <td className="max-w-xs truncate py-2 pr-4 text-neutral-400" title={r.path}>
                    {r.path}
                  </td>
                  <td className={`py-2 pr-4 ${r.rate_limited ? "text-[var(--color-status-error)]" : "text-neutral-400"}`}>
                    {r.rate_limited ? "rate limited" : r.status_code}
                  </td>
                  <td className="py-2 pr-4 whitespace-nowrap text-neutral-500">{new Date(r.created_at).toLocaleString()}</td>
                </tr>
              ))}
            </tbody>
          </table>

          {(requests ?? []).length === 0 && <p className="py-4 text-sm text-neutral-500">No requests over this tunnel yet.</p>}
        </div>
      </Card>
    </div>
  );
}
