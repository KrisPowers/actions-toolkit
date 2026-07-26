import { useEffect, useState } from "react";
import { Box, Plus } from "lucide-react";
import { useRuntimeStatus, useSettings, useSuggestedHostPaths, useUpdateSettings } from "../../hooks/useSettings";
import StatusBadge from "../common/StatusBadge";
import Input from "../common/Input";
import Button from "../common/Button";
import Card from "../common/Card";
import { fieldClass } from "../../lib/fieldClass";
import { cn } from "../../lib/cn";

export default function BucketSettingsCard() {
  const { data: settings } = useSettings();
  const { data: runtimeStatus } = useRuntimeStatus();
  const { data: suggestedHostPaths } = useSuggestedHostPaths();
  const update = useUpdateSettings();

  const [ttlSeconds, setTtlSeconds] = useState("");
  const [cpuLimitMillis, setCpuLimitMillis] = useState("");
  const [memoryLimitMb, setMemoryLimitMb] = useState("");
  const [hostMountsText, setHostMountsText] = useState("");

  useEffect(() => {
    if (!settings) return;
    setTtlSeconds(String(settings.bucket_default_ttl_seconds));
    setCpuLimitMillis(settings.bucket_cpu_limit_millis != null ? String(settings.bucket_cpu_limit_millis) : "");
    setMemoryLimitMb(settings.bucket_memory_limit_mb != null ? String(settings.bucket_memory_limit_mb) : "");
    try {
      const paths: unknown = JSON.parse(settings.bucket_host_mounts_json);
      setHostMountsText(Array.isArray(paths) ? paths.join("\n") : "");
    } catch {
      setHostMountsText("");
    }
  }, [settings]);

  const ttlValue = Number(ttlSeconds);
  const ttlValid = Number.isInteger(ttlValue) && ttlValue > 0;
  const hostMountPaths = hostMountsText
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0);

  // Appends to the textarea only, same as typing it by hand -- still requires the explicit Save
  // below before the sandbox actually grants it anything, one click doesn't skip that approval.
  function addSuggestedPath(path: string) {
    setHostMountsText((current) => (current.trim().length > 0 ? `${current}\n${path}` : path));
  }

  return (
    <Card className="p-5">
      <div className="flex items-center gap-2">
        <Box className="h-4 w-4 text-neutral-500" strokeWidth={2} />
        <h2 className="text-sm font-semibold text-neutral-200">Bucket / sandbox</h2>
      </div>
      <p className="mt-1 text-xs text-neutral-500">
        The native sandbox that runs jobs without a <code>container:</code>, no Docker involved.
      </p>

      <div className="mt-3 flex flex-wrap gap-2">
        <StatusBadge
          status={runtimeStatus?.bucket_available ? "succeeded" : "failed"}
          label={`Bucket ${runtimeStatus?.bucket_available ? "available" : "unavailable"}`}
        />
      </div>
      {runtimeStatus && !runtimeStatus.bucket_available && runtimeStatus.bucket_unavailable_reason && (
        <p className="mt-2 text-xs text-[var(--color-status-error)]">{runtimeStatus.bucket_unavailable_reason}</p>
      )}

      <div className="mt-4">
        <label className="text-xs font-medium text-neutral-400" htmlFor="bucket-ttl">
          Default TTL (seconds)
        </label>
        <Input
          id="bucket-ttl"
          type="number"
          min={1}
          value={ttlSeconds}
          onChange={(e) => setTtlSeconds(e.target.value)}
          className="mt-1.5 w-32"
        />
        <p className="mt-1 text-xs text-neutral-600">How long a sandbox may live before the TTL reaper force-cleans it.</p>
      </div>

      <div className="mt-4 flex gap-4">
        <div>
          <label className="text-xs font-medium text-neutral-400" htmlFor="bucket-cpu">
            CPU limit (millicores)
          </label>
          <Input
            id="bucket-cpu"
            type="number"
            min={0}
            placeholder="unlimited"
            value={cpuLimitMillis}
            onChange={(e) => setCpuLimitMillis(e.target.value)}
            className="mt-1.5 w-28"
          />
        </div>
        <div>
          <label className="text-xs font-medium text-neutral-400" htmlFor="bucket-memory">
            Memory limit (MB)
          </label>
          <Input
            id="bucket-memory"
            type="number"
            min={0}
            placeholder="unlimited"
            value={memoryLimitMb}
            onChange={(e) => setMemoryLimitMb(e.target.value)}
            className="mt-1.5 w-28"
          />
        </div>
      </div>
      <p className="mt-2 text-xs text-neutral-600">
        Saved and available to a future workflow-engine version; not enforced by the sandbox yet. Leave blank for
        unlimited.
      </p>

      <div className="mt-4">
        <label className="text-xs font-medium text-neutral-400" htmlFor="bucket-host-mounts">
          Extra host paths
        </label>
        <textarea
          id="bucket-host-mounts"
          rows={3}
          placeholder={"C:\\Users\\me\\.cargo\nC:\\Users\\me\\.rustup"}
          value={hostMountsText}
          onChange={(e) => setHostMountsText(e.target.value)}
          className={cn(fieldClass(), "mt-1.5 w-full font-mono text-xs")}
        />
        <p className="mt-1 text-xs text-neutral-600">
          One path per line. Granted read-only (Linux: bind mount; Windows: ACL) to every job sandbox, on top of the
          built-in OS defaults, so it needs to cover any toolchain a step's <code>run:</code> command relies on that
          isn't already reachable, e.g. cargo, nvm, or pyenv installed under a user profile.
        </p>
        {suggestedHostPaths && suggestedHostPaths.length > 0 && (
          <div className="mt-2">
            <p className="text-xs text-neutral-600">Detected on this host, not yet allowlisted:</p>
            <div className="mt-1.5 flex flex-wrap gap-1.5">
              {suggestedHostPaths.map((suggestion) => (
                <button
                  key={suggestion.path}
                  type="button"
                  onClick={() => addSuggestedPath(suggestion.path)}
                  className="flex items-center gap-1 rounded-full border border-neutral-700 bg-neutral-900 px-2.5 py-1 text-xs text-neutral-300 hover:border-accent hover:text-neutral-100"
                  title={suggestion.path}
                >
                  <Plus className="h-3 w-3 shrink-0" strokeWidth={2} />
                  {suggestion.label}
                </button>
              ))}
            </div>
          </div>
        )}
      </div>

      <div className="mt-4 border-t border-neutral-800 pt-4">
        <Button
          variant="primary"
          disabled={!ttlValid || update.isPending}
          onClick={() =>
            update.mutate({
              bucket_default_ttl_seconds: ttlValue,
              bucket_cpu_limit_millis: cpuLimitMillis === "" ? 0 : Number(cpuLimitMillis),
              bucket_memory_limit_mb: memoryLimitMb === "" ? 0 : Number(memoryLimitMb),
              bucket_host_mounts_json: JSON.stringify(hostMountPaths),
            })
          }
        >
          {update.isPending ? "Saving…" : "Save"}
        </Button>
        {update.isError && <p className="mt-2 text-xs text-[var(--color-status-error)]">{(update.error as Error).message}</p>}
        {update.isSuccess && <p className="mt-2 text-xs text-neutral-500">Saved.</p>}
      </div>
    </Card>
  );
}
