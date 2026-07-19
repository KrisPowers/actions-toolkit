import { useEffect, useState } from "react";
import { Server } from "lucide-react";
import { useRuntimeStatus, useSettings, useUpdateSettings } from "../../hooks/useSettings";
import StatusBadge from "../common/StatusBadge";
import Input from "../common/Input";
import Button from "../common/Button";
import Card from "../common/Card";

export default function RuntimeSettingsCard() {
  const { data: settings } = useSettings();
  const { data: runtimeStatus } = useRuntimeStatus();
  const update = useUpdateSettings();

  const [bindAddr, setBindAddr] = useState("");
  const [dockerHost, setDockerHost] = useState("");
  const [maxConcurrentJobs, setMaxConcurrentJobs] = useState("");

  useEffect(() => {
    if (!settings) return;
    setBindAddr(settings.bind_addr);
    setDockerHost(settings.docker_host ?? "");
    setMaxConcurrentJobs(String(settings.max_concurrent_jobs));
  }, [settings]);

  const jobsValue = Number(maxConcurrentJobs);
  const jobsValid = Number.isInteger(jobsValue) && jobsValue > 0;

  return (
    <Card className="p-5">
      <div className="flex items-center gap-2">
        <Server className="h-4 w-4 text-neutral-500" strokeWidth={2} />
        <h2 className="text-sm font-semibold text-neutral-200">Runtime settings</h2>
      </div>

      <div className="mt-3 flex flex-wrap gap-2">
        <StatusBadge
          status={runtimeStatus?.docker_available ? "succeeded" : "failed"}
          label={`Docker ${runtimeStatus?.docker_available ? "available" : "unavailable"}`}
        />
        <StatusBadge
          status={runtimeStatus?.bucket_available ? "succeeded" : "failed"}
          label={`Bucket ${runtimeStatus?.bucket_available ? "available" : "unavailable"}`}
        />
      </div>

      <div className="mt-4">
        <label className="text-xs font-medium text-neutral-400">Port</label>
        <p className="mt-1 text-sm text-neutral-300">{settings?.port ?? "–"}</p>
        <p className="mt-1 text-xs text-neutral-600">
          Change with <code className="text-neutral-500">actions-toolkit start --port &lt;n&gt;</code>
        </p>
      </div>

      <div className="mt-4">
        <label className="text-xs font-medium text-neutral-400" htmlFor="bind-addr">
          Bind address
        </label>
        <Input id="bind-addr" value={bindAddr} onChange={(e) => setBindAddr(e.target.value)} placeholder="0.0.0.0" className="mt-1.5 w-full" />
      </div>

      <div className="mt-4">
        <label className="text-xs font-medium text-neutral-400" htmlFor="docker-host">
          Docker host override
        </label>
        <Input
          id="docker-host"
          value={dockerHost}
          onChange={(e) => setDockerHost(e.target.value)}
          placeholder="leave blank to auto-detect"
          className="mt-1.5 w-full"
        />
      </div>

      <div className="mt-4">
        <label className="text-xs font-medium text-neutral-400" htmlFor="max-jobs">
          Max concurrent jobs
        </label>
        <Input
          id="max-jobs"
          type="number"
          min={1}
          value={maxConcurrentJobs}
          onChange={(e) => setMaxConcurrentJobs(e.target.value)}
          className="mt-1.5 w-24"
        />
      </div>
      <p className="mt-2 text-xs text-neutral-600">
        Bind address and Docker host need a restart to apply. Max concurrent jobs applies to the next run.
      </p>

      <div className="mt-4 border-t border-neutral-800 pt-4">
        <Button
          variant="primary"
          disabled={!jobsValid || update.isPending}
          onClick={() =>
            update.mutate({
              bind_addr: bindAddr,
              docker_host: dockerHost,
              max_concurrent_jobs: jobsValue,
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
