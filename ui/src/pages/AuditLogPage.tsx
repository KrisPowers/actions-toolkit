import { useState } from "react";
import { useParams } from "react-router-dom";
import type { LucideIcon } from "lucide-react";
import { ChevronLeft, ChevronRight, FileCode2, GitBranch, History, KeyRound, Play, Settings as SettingsIcon } from "lucide-react";
import { useAuditLog } from "../hooks/useAuditLog";
import Avatar from "../components/common/Avatar";
import Button from "../components/common/Button";
import Card from "../components/common/Card";
import EmptyState from "../components/common/EmptyState";

const PAGE_SIZE = 50;

const ACTION_ICONS: Record<string, LucideIcon> = {
  "workflow.created": FileCode2,
  "workflow.updated": FileCode2,
  "workflow.deleted": FileCode2,
  "workflow.enabled": FileCode2,
  "workflow.disabled": FileCode2,
  "workflow.imported": FileCode2,
  "run.dispatched": Play,
  "repo.connected": GitBranch,
  "repo.synced": GitBranch,
  "repo.webhook_recreated": GitBranch,
  "secret.set": KeyRound,
  "secret.deleted": KeyRound,
};

export default function AuditLogPage() {
  const { repoId } = useParams();
  const [page, setPage] = useState(0);
  const { data: entries, isLoading } = useAuditLog(repoId, page, PAGE_SIZE);

  return (
    <Card className="p-5">
      <div className="flex items-center gap-2">
        <History className="h-4 w-4 text-neutral-500" strokeWidth={2} />
        <h2 className="text-sm font-semibold text-neutral-200">Audit log</h2>
      </div>
      <p className="mt-1 text-xs text-neutral-500">Every run, workflow change, and integration action for this repo.</p>

      {!isLoading && (entries ?? []).length === 0 && (
        <div className="mt-4">
          <EmptyState icon={History} message="Nothing logged yet." />
        </div>
      )}

      {(entries ?? []).length > 0 && (
        <ul className="mt-4 flex flex-col divide-y divide-neutral-900">
          {(entries ?? []).map((e) => {
            const Icon = ACTION_ICONS[e.action] ?? SettingsIcon;
            return (
              <li key={e.id} className="flex items-start gap-3 py-3">
                <Icon className="mt-0.5 h-4 w-4 shrink-0 text-neutral-500" strokeWidth={2} />
                <div className="min-w-0 flex-1">
                  <p className="text-sm text-neutral-200">{e.summary}</p>
                  <div className="mt-0.5 flex items-center gap-1.5 text-xs text-neutral-500">
                    {e.actor_login ? (
                      <>
                        <Avatar login={e.actor_login} size={14} />
                        <span>@{e.actor_login}</span>
                      </>
                    ) : (
                      <span>System</span>
                    )}
                    <span>&middot;</span>
                    <span>{new Date(e.created_at).toLocaleString()}</span>
                  </div>
                </div>
              </li>
            );
          })}
        </ul>
      )}

      <div className="mt-4 flex items-center justify-end gap-2">
        <Button variant="default" size="sm" onClick={() => setPage((p) => Math.max(0, p - 1))} disabled={page === 0}>
          <ChevronLeft className="h-3.5 w-3.5" strokeWidth={2} />
          Newer
        </Button>
        <Button variant="default" size="sm" onClick={() => setPage((p) => p + 1)} disabled={(entries?.length ?? 0) < PAGE_SIZE}>
          Older
          <ChevronRight className="h-3.5 w-3.5" strokeWidth={2} />
        </Button>
      </div>
    </Card>
  );
}
