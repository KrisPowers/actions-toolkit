import { Box, Boxes, Server } from "lucide-react";
import { Link } from "react-router-dom";
import StatusBadge from "../common/StatusBadge";
import { cardClass } from "../common/Card";
import { formatDuration } from "../../lib/duration";
import type { BucketSummary, ResourceSample, Shard, Shell } from "../../api/types";

function formatBytes(bytes: number | null | undefined): string {
  if (bytes == null) return "—";
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(0)} MB`;
}

function formatPercent(value: number | null | undefined): string {
  return value == null ? "—" : `${value.toFixed(0)}%`;
}

function latestFor(samples: ResourceSample[], subjectType: "shell" | "shard", subjectId: string): ResourceSample | null {
  const matches = samples.filter((s) => s.subject_type === subjectType && s.subject_id === subjectId);
  if (matches.length === 0) return null;
  return matches.reduce((a, b) => (a.ts > b.ts ? a : b));
}

function peakFor(samples: ResourceSample[], subjectType: "shell" | "shard", subjectId: string): { cpu: number | null; mem: number | null } {
  const matches = samples.filter((s) => s.subject_type === subjectType && s.subject_id === subjectId);
  const cpu = matches.reduce<number | null>((max, s) => (s.cpu_percent == null ? max : Math.max(max ?? 0, s.cpu_percent)), null);
  const mem = matches.reduce<number | null>((max, s) => (s.memory_bytes == null ? max : Math.max(max ?? 0, s.memory_bytes)), null);
  return { cpu, mem };
}

function NodeRow({ icon: Icon, title, right, children }: { icon: typeof Box; title: React.ReactNode; right?: React.ReactNode; children?: React.ReactNode }) {
  return (
    <div className={cardClass("p-3")}>
      <div className="flex items-center justify-between gap-3">
        <div className="flex min-w-0 items-center gap-2">
          <Icon className="h-4 w-4 shrink-0 text-neutral-500" strokeWidth={2} />
          <div className="min-w-0 truncate text-sm font-medium text-neutral-200">{title}</div>
        </div>
        {right}
      </div>
      {children}
    </div>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="text-[10px] uppercase tracking-wide text-neutral-600">{label}</div>
      <div className="mt-0.5 text-xs text-neutral-300">{value}</div>
    </div>
  );
}

function ShardCard({ shard, samples, shellId }: { shard: Shard; samples: ResourceSample[]; shellId: string }) {
  const own = latestFor(samples, "shard", shard.id);
  const peak = peakFor(samples, "shard", shard.id);
  // Windows has no persisted per-shard accounting handle to read (see the sampler's doc
  // comment), so a shard with no samples of its own falls back to showing its parent shell's
  // numbers for the same window, clearly labeled as approximated rather than isolated.
  const fallback = !own ? latestFor(samples, "shell", shellId) : null;
  const display = own ?? fallback;
  const approximated = !own && !!fallback;

  return (
    <NodeRow
      icon={Box}
      title={<span className="font-mono text-xs">{shard.id.slice(0, 8)}</span>}
      right={<span className="text-xs text-neutral-500">{shard.reaped_at ? "torn down" : "active"}</span>}
    >
      <div className="mt-2 grid grid-cols-2 gap-2 sm:grid-cols-4">
        <Stat label="CPU" value={formatPercent(display?.cpu_percent)} />
        <Stat label="Memory" value={formatBytes(display?.memory_bytes)} />
        <Stat label="Peak CPU" value={formatPercent(own ? peak.cpu : null)} />
        <Stat label="Network" value={shard.network_enabled ? "enabled" : "isolated"} />
      </div>
      {approximated && (
        <div className="mt-2 text-[10px] text-neutral-600">
          Approximated from this shard's parent shell (no isolated OS accounting available on this host).
        </div>
      )}
    </NodeRow>
  );
}

function ShellCard({ node, samples }: { node: { shell: Shell; shards: Shard[] }; samples: ResourceSample[] }) {
  const { shell, shards } = node;
  const peak = peakFor(samples, "shell", shell.id);

  return (
    <div className="flex flex-col gap-2">
      <NodeRow
        icon={Server}
        title={
          <span className="flex items-center gap-2">
            Shell <span className="font-mono text-xs text-neutral-500">{shell.id.slice(0, 8)}</span>
          </span>
        }
        right={
          <div className="flex items-center gap-2">
            {shell.exit_code !== null && <span className="text-xs text-neutral-500">exit {shell.exit_code}</span>}
            <StatusBadge status={shell.status} />
          </div>
        }
      >
        <div className="mt-2 grid grid-cols-2 gap-2 sm:grid-cols-4">
          <Stat label="Duration" value={formatDuration(shell.started_at, shell.finished_at)} />
          <Stat label="Peak CPU" value={formatPercent(peak.cpu)} />
          <Stat label="Peak memory" value={formatBytes(peak.mem)} />
          <Stat label="Cache hit/miss" value={`${shell.cache_hits} / ${shell.cache_misses}`} />
        </div>
      </NodeRow>
      {shards.length > 0 && (
        <div className="ml-6 flex flex-col gap-2 border-l border-neutral-800 pl-4">
          {shards.map((shard) => (
            <ShardCard key={shard.id} shard={shard} samples={samples} shellId={shell.id} />
          ))}
        </div>
      )}
    </div>
  );
}

/**
 * Bucket → Shell(s) → Shard(s) topology tree. Used both on a single run's Backend tab (one
 * shell) and the Bucket detail page (every sibling shell that event triggered).
 */
export default function BackendTopology({
  bucket,
  shells,
  samples,
}: {
  bucket: BucketSummary | null;
  shells: { shell: Shell; shards: Shard[] }[];
  samples: ResourceSample[];
}) {
  if (!bucket) {
    return <p className="text-sm text-neutral-500">No bucket has been created for this run yet.</p>;
  }

  return (
    <div className="flex flex-col gap-2">
      <NodeRow
        icon={Boxes}
        title={
          <span className="flex items-center gap-2">
            Bucket <span className="font-mono text-xs text-neutral-500">{bucket.bucket.id.slice(0, 8)}</span>
          </span>
        }
        right={<StatusBadge status={bucket.bucket.status} />}
      >
        <div className="mt-2 grid grid-cols-2 gap-2 sm:grid-cols-4">
          <Stat label="Trigger" value={bucket.bucket.trigger_kind} />
          <Stat label="Shells" value={String(bucket.shell_count)} />
          <Stat label="Assets cached" value={String(bucket.assets_cached)} />
          <Stat label="Started" value={new Date(bucket.bucket.created_at).toLocaleTimeString()} />
        </div>
        <div className="mt-2">
          <Link to={`/buckets/${bucket.bucket.id}`} className="text-xs text-[var(--color-status-info)] hover:underline">
            View full backend for this trigger →
          </Link>
        </div>
      </NodeRow>
      <div className="ml-6 flex flex-col gap-3 border-l border-neutral-800 pl-4">
        {shells.map((node) => (
          <ShellCard key={node.shell.id} node={node} samples={samples} />
        ))}
        {shells.length === 0 && <p className="text-xs text-neutral-600">No shells recorded yet.</p>}
      </div>
    </div>
  );
}
