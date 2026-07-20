import { Link, useParams } from "react-router-dom";
import { AlertTriangle, Cloud, Globe, Router, RotateCw } from "lucide-react";
import { useRepo, useSyncRepo } from "../../hooks/useRepos";
import { useNetworkInfo, useSettings } from "../../hooks/useSettings";
import Button from "../../components/common/Button";
import Card from "../../components/common/Card";
import WebhookUnreachableBanner from "../../components/common/WebhookUnreachableBanner";
import TunnelQuickActionCard from "../../components/settings/TunnelQuickActionCard";

export default function RepoWebhooksPage() {
  const { repoId } = useParams();
  const { data: repo } = useRepo(repoId);
  const { data: settings } = useSettings();
  const { data: networkInfo } = useNetworkInfo();
  const syncRepo = useSyncRepo();

  if (!repo) return null;

  const port = settings?.port ?? 7890;
  const portForwardUrl = networkInfo?.public_ip
    ? `http://${networkInfo.public_ip}:${networkInfo.port}${networkInfo.webhook_path_template.replace("{repo_id}", repo.id)}`
    : undefined;

  return (
    <div className="flex flex-col gap-5">
      <Card className="p-5">
        <div className="text-sm font-medium text-neutral-200">Current webhook</div>
        <code className="mt-2 block break-all rounded bg-neutral-950 px-2 py-1 text-xs text-neutral-400">{repo.webhook_url}</code>
        {settings?.public_url && (
          <p className="mt-2 text-xs text-neutral-600">
            Instance-wide public URL is set to <code className="text-neutral-400">{settings.public_url}</code>. This
            applies to every connected repo on this instance, not just this one.
          </p>
        )}

        {!repo.webhook_connected ? (
          <div className="mt-3">
            <WebhookUnreachableBanner />
            <p className="mt-2 text-xs text-neutral-600">
              Until that's fixed, this instance automatically polls for new releases every few minutes instead.
            </p>
            <Button
              variant="default"
              size="sm"
              className="mt-2"
              onClick={() => syncRepo.mutate(repo.id)}
              disabled={syncRepo.isPending}
            >
              <RotateCw className={`h-3.5 w-3.5 ${syncRepo.isPending ? "animate-spin" : ""}`} strokeWidth={2} />
              {syncRepo.isPending ? "Syncing…" : "Sync now"}
            </Button>
            {syncRepo.data && (
              <p className="mt-2 text-xs text-neutral-500">
                {syncRepo.data.dispatched ? "Found and dispatched a new release." : "No new release since the last sync."}
              </p>
            )}
          </div>
        ) : (
          <p className="mt-2 text-xs text-[var(--color-status-success)]">GitHub can reach this webhook.</p>
        )}

        <p className="mt-3 border-t border-neutral-800 pt-3 text-xs text-neutral-600">
          Recent deliveries are on the{" "}
          <Link to={`/repos/${repo.id}/events`} className="text-accent hover:underline">
            Flagged Events
          </Link>{" "}
          page.
        </p>
      </Card>

      <div>
        <div className="text-sm font-medium text-neutral-200">Quick actions</div>
        <p className="mt-1 text-xs text-neutral-500">
          GitHub needs a real public URL to call back into this instance. Pick whichever of these matches how you're
          exposing it, or fill in the fields yourself &mdash; nothing here is applied until you click "Use this URL".
        </p>

        <div className="mt-3 grid grid-cols-1 gap-4 xl:grid-cols-2">
          <TunnelQuickActionCard
            icon={Cloud}
            title="Cloudflare Tunnel"
            placeholder="https://random-words.trycloudflare.com"
            repoId={repo.id}
          >
            <p className="mt-1 text-xs text-neutral-500">
              Run this on the machine hosting this instance, then paste the URL it prints:
            </p>
            <code className="mt-2 block break-all rounded bg-neutral-950 px-2 py-1 text-xs text-neutral-400">
              cloudflared tunnel --url http://localhost:{port}
            </code>
          </TunnelQuickActionCard>

          <TunnelQuickActionCard
            icon={Globe}
            title="Other tunnel or reverse proxy"
            placeholder="https://your-tunnel-url.example"
            repoId={repo.id}
          >
            <p className="mt-1 text-xs text-neutral-500">
              ngrok, Tailscale Funnel, a reverse proxy you already run &mdash; paste whatever public URL it gives you.
            </p>
          </TunnelQuickActionCard>

          <TunnelQuickActionCard
            icon={Router}
            title="Manual port forward"
            placeholder="http://your-public-ip:port"
            initialUrl={portForwardUrl}
            repoId={repo.id}
          >
            <p className="mt-1 text-xs text-neutral-500">
              Forward port <code className="text-neutral-400">{port}</code> on your router to this machine, then use
              the assembled URL below.
            </p>
            <dl className="mt-2 flex flex-col gap-1 text-xs">
              <div className="flex gap-2">
                <dt className="w-24 shrink-0 text-neutral-600">Public IP</dt>
                <dd className="text-neutral-400">
                  {networkInfo === undefined
                    ? "detecting…"
                    : (networkInfo.public_ip ?? "couldn't detect automatically — check your router's WAN page")}
                </dd>
              </div>
              <div className="flex gap-2">
                <dt className="w-24 shrink-0 text-neutral-600">Port</dt>
                <dd className="text-neutral-400">{port}</dd>
              </div>
              <div className="flex gap-2">
                <dt className="w-24 shrink-0 text-neutral-600">Path</dt>
                <dd className="break-all text-neutral-400">{networkInfo?.webhook_path_template.replace("{repo_id}", repo.id) ?? "…"}</dd>
              </div>
            </dl>
            <p className="mt-2 flex items-start gap-1.5 text-xs text-[var(--color-status-warning)]">
              <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" strokeWidth={2} />
              Plain port-forwarding has no TLS termination in front of it &mdash; the URL above is http, not https.
            </p>
          </TunnelQuickActionCard>
        </div>

        <p className="mt-3 text-xs text-neutral-600">
          If "Use this URL" fails with a permission error, make sure your GitHub App has the{" "}
          <span className="text-neutral-400">Webhooks (read &amp; write)</span> repository permission enabled.
        </p>
      </div>
    </div>
  );
}
