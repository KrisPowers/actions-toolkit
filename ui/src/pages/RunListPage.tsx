import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { AlertTriangle, ChevronDown, ChevronRight, Clock, PlayCircle, ShieldCheck, ShieldX } from "lucide-react";
import { useRuns, useRunsForEvent } from "../hooks/useRuns";
import { useRepoWebhookEvents } from "../hooks/useRepos";
import type { WebhookEvent } from "../api/types";
import StatusBadge from "../components/common/StatusBadge";
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

function TriggeredRuns({ eventId }: { eventId: string }) {
  const { data: runs, isLoading } = useRunsForEvent(eventId);

  if (isLoading) return <p className="mt-2 text-xs text-neutral-600">Loading runs…</p>;
  if (!runs || runs.length === 0) return <p className="mt-2 text-xs text-neutral-600">No workflow runs came from this event.</p>;

  return (
    <div className="mt-2 flex flex-col gap-1.5">
      {runs.map((run) => (
        <Link
          key={run.id}
          to={`/runs/${run.id}`}
          className="flex items-center justify-between gap-2 rounded border border-neutral-800 bg-neutral-950/40 px-2.5 py-1.5 hover:border-neutral-700"
        >
          <span className="min-w-0 truncate text-xs text-neutral-300">{run.trigger_event}{run.ref_name ? ` · ${run.ref_name}` : ""}</span>
          <StatusBadge status={run.status} />
        </Link>
      ))}
    </div>
  );
}

function RunsColumn({ repoId }: { repoId: string | undefined }) {
  const { data: runs, isLoading } = useRuns(repoId, 100);

  return (
    <div className="flex min-h-0 flex-col">
      <h2 className="mb-2 text-sm font-semibold text-neutral-200">Workflow runs</h2>
      <div className="min-h-0 flex-1 overflow-y-auto">
        {isLoading && <p className="text-sm text-neutral-500">Loading…</p>}
        <div className={listCardClass()}>
          {(runs ?? []).map((run) => (
            <Link key={run.id} to={`/runs/${run.id}`} className="flex items-center justify-between px-4 py-3 hover:bg-neutral-800/50">
              <div>
                <div className="text-sm text-neutral-200">
                  {run.trigger_event}
                  {run.ref_name ? (
                    <>
                      {" · "}
                      <span className="font-mono">{run.ref_name}</span>
                    </>
                  ) : (
                    ""
                  )}
                </div>
                <div className="mt-0.5 flex items-center gap-1 text-xs text-neutral-500">
                  <Clock className="h-3 w-3" strokeWidth={2} />
                  {new Date(run.created_at).toLocaleString()}
                </div>
              </div>
              <StatusBadge status={run.status} />
            </Link>
          ))}
          {(runs ?? []).length === 0 && !isLoading && <EmptyState icon={PlayCircle} message="No runs yet." />}
        </div>
      </div>
    </div>
  );
}

function EventsColumn({ repoId }: { repoId: string | undefined }) {
  const { data: events, isLoading } = useRepoWebhookEvents(repoId);
  const [expandedId, setExpandedId] = useState<string | null>(null);

  return (
    <div className="flex min-h-0 flex-col">
      <h2 className="mb-2 text-sm font-semibold text-neutral-200">Webhook events</h2>
      <div className="min-h-0 flex-1 overflow-y-auto">
        {isLoading && <p className="text-sm text-neutral-500">Loading…</p>}
        <div className={listCardClass()}>
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
                  <div className="mx-4 mb-3">
                    <pre className="max-h-80 overflow-auto rounded-md border border-neutral-800 bg-neutral-950 p-3 text-xs text-neutral-400">
                      {prettyPayload(event)}
                    </pre>
                    <div className="mt-2">
                      <h3 className="text-xs font-medium text-neutral-400">Triggered runs</h3>
                      <TriggeredRuns eventId={event.id} />
                    </div>
                  </div>
                )}
              </div>
            );
          })}
          {(events ?? []).length === 0 && !isLoading && (
            <EmptyState icon={AlertTriangle} message="No webhook deliveries received for this repo yet." />
          )}
        </div>
      </div>
    </div>
  );
}

export default function RunListPage() {
  const { repoId } = useParams();

  return (
    <div className="flex h-full flex-col">
      <div className="pb-3">
        <PageHeader title="Runs" subtitle="Workflow runs on the left, the webhook events that triggered them on the right." />
      </div>
      <div className="grid min-h-0 flex-1 grid-cols-1 gap-6 lg:grid-cols-2">
        <RunsColumn repoId={repoId} />
        <EventsColumn repoId={repoId} />
      </div>
    </div>
  );
}
