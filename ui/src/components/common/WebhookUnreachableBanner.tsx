import { AlertTriangle } from "lucide-react";

/**
 * A repo can be fully "connected" in the UI with a valid webhook_url and still have no actual
 * way for GitHub to reach this instance. `github_hook_id` stays null when hook creation never
 * ran or failed silently behind it. In that state, event triggers (push, pull_request, release)
 * never fire, with nothing else in the UI to say so: no error, no run ever gets created. This
 * makes that state visible instead of silent.
 */
export default function WebhookUnreachableBanner() {
  return (
    <div className="flex items-start gap-2 rounded-md border border-[var(--color-status-warning)]/30 bg-[var(--color-status-warning)]/5 px-3 py-2.5 text-sm text-[var(--color-status-warning)]">
      <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" strokeWidth={2} />
      <p>
        GitHub has no webhook registered for this repo, so event triggers (push, pull request, release) won't fire,
        only manual "Run now" dispatch works. This usually means the connect step ran before this instance was
        publicly reachable. See{" "}
        <a
          href="https://github.com/KrisPowers/actions-toolkit#exposing-your-webhook"
          target="_blank"
          rel="noreferrer"
          className="underline hover:no-underline"
        >
          Exposing your webhook
        </a>{" "}
        in the README, then disconnect and reconnect this repo.
      </p>
    </div>
  );
}
