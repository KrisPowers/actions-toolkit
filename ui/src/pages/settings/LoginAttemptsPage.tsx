import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { ChevronLeft, ChevronRight, History } from "lucide-react";
import { authApi } from "../../api/auth";
import Avatar from "../../components/common/Avatar";
import Button from "../../components/common/Button";
import Card from "../../components/common/Card";

const PAGE_SIZE = 25;

const OUTCOME_STYLES: Record<string, string> = {
  approved: "text-[var(--color-status-success)]",
  pending: "text-neutral-400",
  restricted: "text-[var(--color-status-error)]",
  denied: "text-neutral-500",
  rate_limited: "text-[var(--color-status-error)]",
  failed: "text-[var(--color-status-error)]",
};

/**
 * Raw IP + user agent only, no geo/device parsing library or external lookup: this instance
 * is self-hosted and shouldn't depend on (or leak login IPs to) a third-party geo-IP service.
 */
export default function LoginAttemptsPage() {
  const [page, setPage] = useState(0);
  const { data: events, isLoading } = useQuery({
    queryKey: ["login-events", page],
    queryFn: () => authApi.listLoginEvents(PAGE_SIZE, page * PAGE_SIZE),
  });

  return (
    <Card className="p-5">
      <div className="flex items-center gap-2">
        <History className="h-4 w-4 text-neutral-500" strokeWidth={2} />
        <h2 className="text-sm font-semibold text-neutral-200">Login attempts</h2>
      </div>
      <p className="mt-1 text-xs text-neutral-500">Every GitHub login attempt, approved or not.</p>

      <div className="mt-4 overflow-x-auto">
        <table className="w-full text-left text-sm">
          <thead>
            <tr className="border-b border-neutral-800 text-xs text-neutral-500">
              <th className="py-2 pr-4 font-medium">User</th>
              <th className="py-2 pr-4 font-medium">Outcome</th>
              <th className="py-2 pr-4 font-medium">IP address</th>
              <th className="py-2 pr-4 font-medium">Device</th>
              <th className="py-2 pr-4 font-medium">Time</th>
            </tr>
          </thead>
          <tbody>
            {(events ?? []).map((e) => (
              <tr key={e.id} className="border-b border-neutral-900">
                <td className="py-2 pr-4">
                  {e.github_login ? (
                    <div className="flex items-center gap-2">
                      <Avatar login={e.github_login} size={20} />
                      <span className="text-neutral-200">@{e.github_login}</span>
                    </div>
                  ) : (
                    <span className="text-neutral-500">unknown</span>
                  )}
                </td>
                <td className={`py-2 pr-4 capitalize ${OUTCOME_STYLES[e.outcome] ?? "text-neutral-400"}`}>{e.outcome.replace("_", " ")}</td>
                <td className="py-2 pr-4 text-neutral-400">{e.ip_address ?? "—"}</td>
                <td className="max-w-xs truncate py-2 pr-4 text-neutral-500" title={e.user_agent ?? undefined}>
                  {e.user_agent ?? "—"}
                </td>
                <td className="py-2 pr-4 whitespace-nowrap text-neutral-500">{new Date(e.created_at).toLocaleString()}</td>
              </tr>
            ))}
          </tbody>
        </table>

        {!isLoading && (events ?? []).length === 0 && <p className="py-4 text-sm text-neutral-500">No login attempts yet.</p>}
      </div>

      <div className="mt-4 flex items-center justify-end gap-2">
        <Button variant="default" size="sm" onClick={() => setPage((p) => Math.max(0, p - 1))} disabled={page === 0}>
          <ChevronLeft className="h-3.5 w-3.5" strokeWidth={2} />
          Newer
        </Button>
        <Button variant="default" size="sm" onClick={() => setPage((p) => p + 1)} disabled={(events?.length ?? 0) < PAGE_SIZE}>
          Older
          <ChevronRight className="h-3.5 w-3.5" strokeWidth={2} />
        </Button>
      </div>
    </Card>
  );
}
