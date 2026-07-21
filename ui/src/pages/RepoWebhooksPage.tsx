import { useState } from "react";
import type { ComponentType } from "react";
import { Link, useParams } from "react-router-dom";
import { AlertTriangle, CheckCircle2, Cloud, Globe, Loader2, Network, Router, RotateCw, X } from "lucide-react";
import { useRepo, useSyncRepo } from "../hooks/useRepos";
import {
  useCloudflareTunnelStatus,
  useNetworkInfo,
  useSettings,
  useStartCloudflareTunnel,
  useStartTailscaleTunnel,
  useTailscaleTunnelStatus,
  useTunnelAvailability,
} from "../hooks/useSettings";
import type { CloudflareTunnelState, TailscaleTunnelState } from "../api/types";
import Button from "../components/common/Button";
import Card from "../components/common/Card";
import InfoTooltip from "../components/common/InfoTooltip";
import Modal from "../components/common/Modal";
import PageHeader from "../components/common/PageHeader";
import WebhookUnreachableBanner from "../components/common/WebhookUnreachableBanner";
import WebhookUrlField from "../components/webhooks/WebhookUrlField";

type Method = "cloudflare" | "tailscale" | "tunnel" | "manual";

const METHODS: {
  id: Method;
  label: string;
  icon: ComponentType<{ className?: string; strokeWidth?: number }>;
  blurb: string;
  requiresBinary?: "cloudflared" | "tailscale";
}[] = [
  { id: "cloudflare", label: "Cloudflare Tunnel", icon: Cloud, blurb: "One click, no router access needed.", requiresBinary: "cloudflared" },
  { id: "tailscale", label: "Tailscale Funnel", icon: Network, blurb: "One click, exposes this instance over your tailnet.", requiresBinary: "tailscale" },
  { id: "tunnel", label: "Other tunnel", icon: Globe, blurb: "Already running ngrok or your own reverse proxy?" },
  { id: "manual", label: "Port forward", icon: Router, blurb: "Forward a port on your router. No HTTPS." },
];

function ModalHeader({
  icon: Icon,
  title,
  onClose,
}: {
  icon: ComponentType<{ className?: string; strokeWidth?: number }>;
  title: string;
  onClose: () => void;
}) {
  return (
    <div className="flex items-center justify-between">
      <div className="flex items-center gap-2">
        <Icon className="h-5 w-5 text-neutral-400" strokeWidth={2} />
        <div className="text-sm font-medium text-neutral-200">{title}</div>
      </div>
      <button onClick={onClose} className="flex h-7 w-7 items-center justify-center rounded-md text-neutral-500 hover:bg-neutral-800 hover:text-neutral-200">
        <X className="h-4 w-4" strokeWidth={2} />
      </button>
    </div>
  );
}

/**
 * Shared start/status block for the one-click tunnel modals (Cloudflare, Tailscale). Once the
 * tunnel is actually running there's no live button to show, a disabled "Tunnel running" button
 * sitting next to a redundant checkmark was the old, confusing layout, so this collapses running
 * state down to a single status line instead.
 */
function TunnelControl({
  status,
  onStart,
  starting,
  installed,
  binaryLabel,
}: {
  status: CloudflareTunnelState | TailscaleTunnelState | undefined;
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

export default function RepoWebhooksPage() {
  const { repoId } = useParams();
  const { data: repo } = useRepo(repoId);
  const { data: settings } = useSettings();
  const { data: networkInfo } = useNetworkInfo();
  const { data: tunnelAvailability } = useTunnelAvailability();
  const syncRepo = useSyncRepo();
  const [openMethod, setOpenMethod] = useState<Method | null>(null);

  const { data: cloudflareStatus } = useCloudflareTunnelStatus();
  const startCloudflareTunnel = useStartCloudflareTunnel();
  const { data: tailscaleStatus } = useTailscaleTunnelStatus();
  const startTailscaleTunnel = useStartTailscaleTunnel();

  if (!repo) return null;

  const port = settings?.port ?? 7890;
  const portForwardUrl = networkInfo?.public_ip
    ? `http://${networkInfo.public_ip}:${networkInfo.port}${networkInfo.webhook_path_template.replace("{repo_id}", repo.id)}`
    : undefined;
  const cloudflareUrl = cloudflareStatus?.status === "running" ? cloudflareStatus.url : undefined;
  const tailscaleUrl = tailscaleStatus?.status === "running" ? tailscaleStatus.url : undefined;

  function isAvailable(requiresBinary?: "cloudflared" | "tailscale") {
    if (!requiresBinary || !tunnelAvailability) return true;
    return requiresBinary === "cloudflared" ? tunnelAvailability.cloudflared_available : tunnelAvailability.tailscale_available;
  }

  return (
    <div className="flex w-full flex-col gap-5">
      <PageHeader title="Webhooks" subtitle="How GitHub delivers push, pull request, and release events to this instance." />

      <Card className="p-5">
        <div className="flex items-start gap-3">
          {repo.webhook_connected ? (
            <CheckCircle2 className="mt-0.5 h-5 w-5 shrink-0 text-[var(--color-status-success)]" strokeWidth={2} />
          ) : (
            <AlertTriangle className="mt-0.5 h-5 w-5 shrink-0 text-[var(--color-status-warning)]" strokeWidth={2} />
          )}
          <div className="min-w-0 flex-1">
            <div className="text-sm font-medium text-neutral-200">
              {repo.webhook_connected ? "GitHub can reach this webhook" : "GitHub can't reach this webhook"}
            </div>
            <code className="mt-2 block w-fit max-w-full break-all rounded bg-neutral-950 px-2 py-1 text-xs text-neutral-400">
              {repo.webhook_url}
            </code>
            {settings?.public_url && (
              <div className="mt-2 flex items-center gap-1 text-xs text-neutral-600">
                Instance-wide public URL: <code className="text-neutral-400">{settings.public_url}</code>
                <InfoTooltip text="This applies to every connected repo on this instance, not just the one you're looking at." />
              </div>
            )}

            {!repo.webhook_connected && (
              <div className="mt-3">
                <WebhookUnreachableBanner />
                <div className="mt-2 flex items-center gap-2">
                  <Button variant="default" size="sm" onClick={() => syncRepo.mutate(repo.id)} disabled={syncRepo.isPending}>
                    <RotateCw className={`h-3.5 w-3.5 ${syncRepo.isPending ? "animate-spin" : ""}`} strokeWidth={2} />
                    {syncRepo.isPending ? "Syncing…" : "Sync now"}
                  </Button>
                  {syncRepo.data && (
                    <span className="text-xs text-neutral-500">
                      {syncRepo.data.dispatched ? "Found and dispatched a new release." : "No new release since the last sync."}
                    </span>
                  )}
                </div>
                <p className="mt-2 text-xs text-neutral-600">
                  Until that's fixed, this instance polls for new releases every few minutes instead.
                </p>
              </div>
            )}
          </div>
        </div>

        <p className="mt-4 border-t border-neutral-800 pt-3 text-xs text-neutral-600">
          Recent deliveries are on the{" "}
          <Link to={`/repos/${repo.id}/events`} className="text-accent hover:underline">
            Flagged Events
          </Link>{" "}
          page.
        </p>
      </Card>

      <div>
        <div className="mb-3 flex items-center gap-1">
          <div className="text-sm font-medium text-neutral-200">Point GitHub at this instance</div>
          <InfoTooltip text="GitHub needs a real public URL to call back into this instance. Pick whichever matches how you're exposing it. Nothing is applied until you confirm inside." />
        </div>

        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
          {METHODS.map(({ id, label, icon: Icon, blurb, requiresBinary }) => {
            const available = isAvailable(requiresBinary);
            return (
              <button
                key={id}
                onClick={() => available && setOpenMethod(id)}
                disabled={!available}
                aria-disabled={!available}
                className="flex flex-col items-start gap-2 rounded-md border border-neutral-800 bg-neutral-900 p-5 text-left transition-colors enabled:hover:border-neutral-700 enabled:hover:bg-neutral-800/40 disabled:cursor-not-allowed disabled:opacity-40"
              >
                <Icon className="h-6 w-6 text-neutral-400" strokeWidth={2} />
                <div className="text-sm font-medium text-neutral-200">{label}</div>
                <p className="text-xs text-neutral-500">{available ? blurb : "Not installed on this machine."}</p>
              </button>
            );
          })}
        </div>
      </div>

      <div className="flex items-center gap-1 text-xs text-neutral-600">
        Permission errors when confirming a URL?
        <InfoTooltip text='Your GitHub App needs the "Webhooks (read & write)" repository permission enabled. Enable it, then try again.' />
      </div>

      <Modal open={openMethod === "cloudflare"} onClose={() => setOpenMethod(null)} className="max-w-xl">
        <ModalHeader icon={Cloud} title="Cloudflare Tunnel" onClose={() => setOpenMethod(null)} />
        <div className="mt-3 flex items-start gap-1.5">
          <p className="text-xs text-neutral-500">
            Starts a tunnel to this instance and fills in its public URL automatically, no terminal, no copy-pasting.
          </p>
          <InfoTooltip text="This runs the cloudflared binary on the machine hosting this instance and reads back the URL it's assigned, so nothing manual is needed." />
        </div>

        <TunnelControl
          status={cloudflareStatus}
          onStart={() => startCloudflareTunnel.mutate()}
          starting={startCloudflareTunnel.isPending}
          installed={tunnelAvailability?.cloudflared_available}
          binaryLabel="cloudflared"
        />

        <WebhookUrlField repoId={repo.id} placeholder="https://random-words.trycloudflare.com" initialUrl={cloudflareUrl} className="mt-3" />
      </Modal>

      <Modal open={openMethod === "tailscale"} onClose={() => setOpenMethod(null)} className="max-w-xl">
        <ModalHeader icon={Network} title="Tailscale Funnel" onClose={() => setOpenMethod(null)} />
        <div className="mt-3 flex items-start gap-1.5">
          <p className="text-xs text-neutral-500">
            Starts a Tailscale Funnel for this instance and fills in its public URL automatically, no terminal, no copy-pasting.
          </p>
          <InfoTooltip text="This runs `tailscale funnel` on the machine hosting this instance and reads back the URL it's assigned. Funnel needs to be enabled for your tailnet in the Tailscale admin console first." />
        </div>

        <TunnelControl
          status={tailscaleStatus}
          onStart={() => startTailscaleTunnel.mutate()}
          starting={startTailscaleTunnel.isPending}
          installed={tunnelAvailability?.tailscale_available}
          binaryLabel="tailscale"
        />

        <WebhookUrlField repoId={repo.id} placeholder="https://your-machine.your-tailnet.ts.net" initialUrl={tailscaleUrl} className="mt-3" />
      </Modal>

      <Modal open={openMethod === "tunnel"} onClose={() => setOpenMethod(null)} className="max-w-xl">
        <ModalHeader icon={Globe} title="Other tunnel" onClose={() => setOpenMethod(null)} />
        <div className="mt-3 flex items-start gap-1.5">
          <p className="text-xs text-neutral-500">Already running ngrok or your own reverse proxy? Paste its public URL below.</p>
          <InfoTooltip text={`Example: ngrok http ${port}, then paste the https://*.ngrok-free.app URL it prints.`} />
        </div>
        <WebhookUrlField repoId={repo.id} placeholder="https://your-tunnel-url.example" className="mt-3" />
      </Modal>

      <Modal open={openMethod === "manual"} onClose={() => setOpenMethod(null)} className="max-w-xl">
        <ModalHeader icon={Router} title="Port forward" onClose={() => setOpenMethod(null)} />
        <div className="mt-3 flex items-start gap-1.5">
          <p className="text-xs text-neutral-500">
            Forward port <code className="text-neutral-400">{port}</code> on your router to this machine, then confirm the assembled URL below.
          </p>
          <InfoTooltip text="Log into your router's admin page, find Port Forwarding, and map an external port to this machine's local IP and port. Exact steps vary by router." />
        </div>
        <dl className="mt-3 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1.5 text-xs">
          <dt className="text-neutral-600">Public IP</dt>
          <dd className="text-neutral-400">
            {networkInfo === undefined ? "detecting…" : (networkInfo.public_ip ?? "couldn't detect automatically, check your router's WAN page")}
          </dd>
          <dt className="text-neutral-600">Port</dt>
          <dd className="text-neutral-400">{port}</dd>
          <dt className="text-neutral-600">Path</dt>
          <dd className="break-all text-neutral-400">{networkInfo?.webhook_path_template.replace("{repo_id}", repo.id) ?? "…"}</dd>
        </dl>
        <p className="mt-3 flex items-start gap-1.5 text-xs text-[var(--color-status-warning)]">
          <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" strokeWidth={2} />
          Plain port-forwarding has no TLS termination in front of it. The URL below is http, not https.
        </p>
        <WebhookUrlField repoId={repo.id} placeholder="http://your-public-ip:port" initialUrl={portForwardUrl} className="mt-3" />
      </Modal>
    </div>
  );
}
