import { useState } from "react";
import { useParams } from "react-router-dom";
import { AlertTriangle, ChevronDown, ChevronRight, ShieldCheck, ShieldX } from "lucide-react";
import { useRepoWebhookEvents } from "../hooks/useRepos";
import type { WebhookEvent } from "../api/types";
import PageHeader from "../components/common/PageHeader";
import { listCardClass } from "../components/common/Card";
import EmptyState from "../components/common/EmptyState";

function matchedCount(event: WebhookEvent): number {
  try {
    const ids = JSON.parse(event.matched_workflow_ids);
    return Array.isArray(ids) ? ids.length : 0;
  } catch {
    return 0;
  }
}

function prettyPayload(event: WebhookEvent): string {
  try {
    return JSON.stringify(JSON.parse(event.payload_json), null, 2);
  } catch {
    return event.payload_json;
  }
}

export default function RepoEventsPage() {
  const { repoId } = useParams();
  const { data: events, isLoading } = useRepoWebhookEvents(repoId);
  const [expandedId, setExpandedId] = useState<string | null>(null);

  return (
    <div>
      <PageHeader
        title="Flagged events"
        subtitle="Recent webhook deliveries from GitHub. Rejected signatures or deliveries that matched no workflow are flagged, since those are the ones most likely to need attention."
      />

      {isLoading && <p className="mt-6 text-sm text-neutral-500">Loading…</p>}

      <div className={listCardClass("mt-4")}>
        {(events ?? []).map((event) => {
          const flagged = !event.signature_valid || matchedCount(event) === 0;
          const expanded = expandedId === event.id;
          return (
            <div key={event.id} className={flagged ? "border-l-2 border-l-[var(--color-status-warning)]" : ""}>
              <button
                type="button"
                onClick={() => setExpandedId(expanded ? null : event.id)}
                className="flex w-full items-center justify-between gap-3 px-4 py-3 text-left hover:bg-neutral-800/50"
              >
                <div className="flex min-w-0 items-center gap-2">
                  {expanded ? (
                    <ChevronDown className="h-3.5 w-3.5 shrink-0 text-neutral-500" strokeWidth={2} />
                  ) : (
                    <ChevronRight className="h-3.5 w-3.5 shrink-0 text-neutral-500" strokeWidth={2} />
                  )}
                  {flagged && <AlertTriangle className="h-3.5 w-3.5 shrink-0 text-[var(--color-status-warning)]" strokeWidth={2} />}
                  <div className="min-w-0">
                    <div className="text-sm text-neutral-200">{event.github_event}</div>
                    <div className="mt-0.5 text-xs text-neutral-500">{new Date(event.received_at).toLocaleString()}</div>
                  </div>
                </div>
                <div className="flex shrink-0 items-center gap-3 text-xs">
                  <span className="text-neutral-500">
                    {matchedCount(event)} workflow{matchedCount(event) === 1 ? "" : "s"} matched
                  </span>
                  {event.signature_valid ? (
                    <span className="inline-flex items-center gap-1 text-[var(--color-status-success)]">
                      <ShieldCheck className="h-3.5 w-3.5" strokeWidth={2} />
                      valid signature
                    </span>
                  ) : (
                    <span className="inline-flex items-center gap-1 text-[var(--color-status-error)]">
                      <ShieldX className="h-3.5 w-3.5" strokeWidth={2} />
                      invalid signature
                    </span>
                  )}
                </div>
              </button>
              {expanded && (
                <pre className="mx-4 mb-3 max-h-80 overflow-auto rounded-md border border-neutral-800 bg-neutral-950 p-3 text-xs text-neutral-400">
                  {prettyPayload(event)}
                </pre>
              )}
            </div>
          );
        })}
        {(events ?? []).length === 0 && !isLoading && (
          <EmptyState icon={AlertTriangle} message="No webhook deliveries received for this repo yet." />
        )}
      </div>
    </div>
  );
}
