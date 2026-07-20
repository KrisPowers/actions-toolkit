import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { AlertTriangle, CheckCircle2, Cloud, Globe, Router, RotateCw } from "lucide-react";
import { useRepo, useSyncRepo } from "../hooks/useRepos";
import { useNetworkInfo, useSettings } from "../hooks/useSettings";
import Button from "../components/common/Button";
import Card from "../components/common/Card";
import PageHeader from "../components/common/PageHeader";
import { TabButton, TabList } from "../components/common/Tabs";
import WebhookUnreachableBanner from "../components/common/WebhookUnreachableBanner";
import WebhookUrlField from "../components/webhooks/WebhookUrlField";

type Method = "cloudflare" | "tunnel" | "manual";

export default function RepoWebhooksPage() {
  const { repoId } = useParams();
  const { data: repo } = useRepo(repoId);
  const { data: settings } = useSettings();
  const { data: networkInfo } = useNetworkInfo();
  const syncRepo = useSyncRepo();
  const [method, setMethod] = useState<Method>("cloudflare");

  if (!repo) return null;

  const port = settings?.port ?? 7890;
  const portForwardUrl = networkInfo?.public_ip
    ? `http://${networkInfo.public_ip}:${networkInfo.port}${networkInfo.webhook_path_template.replace("{repo_id}", repo.id)}`
    : undefined;

  return (
    <div className="flex max-w-3xl flex-col gap-5">
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
            <code className="mt-2 block break-all rounded bg-neutral-950 px-2 py-1 text-xs text-neutral-400">{repo.webhook_url}</code>
            {settings?.public_url && (
              <p className="mt-2 text-xs text-neutral-600">
                Instance-wide public URL is set to <code className="text-neutral-400">{settings.public_url}</code>, which applies to
                every connected repo on this instance, not just this one.
              </p>
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

      <Card className="p-0">
        <div className="px-5 pt-4">
          <div className="text-sm font-medium text-neutral-200">Point GitHub at this instance</div>
          <p className="mt-1 pb-3 text-xs text-neutral-500">
            GitHub needs a real public URL to call back into this instance. Pick whichever matches how you're exposing it &mdash;
            nothing is applied until you click "Use this URL".
          </p>
          <TabList>
            <TabButton active={method === "cloudflare"} onClick={() => setMethod("cloudflare")} icon={Cloud}>
              Cloudflare Tunnel
            </TabButton>
            <TabButton active={method === "tunnel"} onClick={() => setMethod("tunnel")} icon={Globe}>
              Other tunnel
            </TabButton>
            <TabButton active={method === "manual"} onClick={() => setMethod("manual")} icon={Router}>
              Port forward
            </TabButton>
          </TabList>
        </div>

        <div className="p-5">
          {method === "cloudflare" && (
            <>
              <p className="text-xs text-neutral-500">Run this on the machine hosting this instance, then paste the URL it prints:</p>
              <code className="mt-2 block break-all rounded bg-neutral-950 px-2 py-1 text-xs text-neutral-400">
                cloudflared tunnel --url http://localhost:{port}
              </code>
              <WebhookUrlField repoId={repo.id} placeholder="https://random-words.trycloudflare.com" className="mt-3" />
            </>
          )}

          {method === "tunnel" && (
            <>
              <p className="text-xs text-neutral-500">
                ngrok, Tailscale Funnel, a reverse proxy you already run &mdash; paste whatever public URL it gives you.
              </p>
              <WebhookUrlField repoId={repo.id} placeholder="https://your-tunnel-url.example" className="mt-3" />
            </>
          )}

          {method === "manual" && (
            <>
              <p className="text-xs text-neutral-500">
                Forward port <code className="text-neutral-400">{port}</code> on your router to this machine, then use the assembled
                URL below.
              </p>
              <dl className="mt-3 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1.5 text-xs">
                <dt className="text-neutral-600">Public IP</dt>
                <dd className="text-neutral-400">
                  {networkInfo === undefined
                    ? "detecting…"
                    : (networkInfo.public_ip ?? "couldn't detect automatically — check your router's WAN page")}
                </dd>
                <dt className="text-neutral-600">Port</dt>
                <dd className="text-neutral-400">{port}</dd>
                <dt className="text-neutral-600">Path</dt>
                <dd className="break-all text-neutral-400">{networkInfo?.webhook_path_template.replace("{repo_id}", repo.id) ?? "…"}</dd>
              </dl>
              <p className="mt-3 flex items-start gap-1.5 text-xs text-[var(--color-status-warning)]">
                <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" strokeWidth={2} />
                Plain port-forwarding has no TLS termination in front of it &mdash; the URL below is http, not https.
              </p>
              <WebhookUrlField repoId={repo.id} placeholder="http://your-public-ip:port" initialUrl={portForwardUrl} className="mt-3" />
            </>
          )}
        </div>
      </Card>

      <p className="text-xs text-neutral-600">
        If "Use this URL" fails with a permission error, make sure your GitHub App has the{" "}
        <span className="text-neutral-400">Webhooks (read &amp; write)</span> repository permission enabled.
      </p>
    </div>
  );
}
