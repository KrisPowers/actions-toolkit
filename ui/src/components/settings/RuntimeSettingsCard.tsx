import { useEffect, useState } from "react";
import { useSettings, useUpdateSettings } from "../../hooks/useSettings";

export default function RuntimeSettingsCard() {
  const { data: settings } = useSettings();
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
    <div className="rounded-lg border border-neutral-800 bg-neutral-900 p-5">
      <h2 className="text-sm font-semibold text-neutral-200">Runtime settings</h2>
      <p className="mt-1 text-xs text-neutral-500">
        Bind address and Docker host changes require restarting actions-toolkit to take effect. Max concurrent jobs
        applies to the next workflow run.
      </p>

      <div className="mt-4">
        <label className="text-xs font-medium text-neutral-400">Port</label>
        <p className="mt-1 text-sm text-neutral-300">{settings?.port ?? "–"}</p>
        <p className="mt-1 text-xs text-neutral-600">
          Change with: <code className="text-neutral-500">actions-toolkit start --port &lt;n&gt;</code>
        </p>
      </div>

      <div className="mt-4">
        <label className="text-xs font-medium text-neutral-400" htmlFor="bind-addr">
          Bind address
        </label>
        <input
          id="bind-addr"
          value={bindAddr}
          onChange={(e) => setBindAddr(e.target.value)}
          placeholder="0.0.0.0"
          className="mt-1.5 w-full rounded-md border border-neutral-700 bg-neutral-950 px-2.5 py-1.5 text-sm text-neutral-100"
        />
      </div>

      <div className="mt-4">
        <label className="text-xs font-medium text-neutral-400" htmlFor="docker-host">
          Docker host override
        </label>
        <input
          id="docker-host"
          value={dockerHost}
          onChange={(e) => setDockerHost(e.target.value)}
          placeholder="leave blank to auto-detect"
          className="mt-1.5 w-full rounded-md border border-neutral-700 bg-neutral-950 px-2.5 py-1.5 text-sm text-neutral-100"
        />
      </div>

      <div className="mt-4">
        <label className="text-xs font-medium text-neutral-400" htmlFor="max-jobs">
          Max concurrent jobs
        </label>
        <input
          id="max-jobs"
          type="number"
          min={1}
          value={maxConcurrentJobs}
          onChange={(e) => setMaxConcurrentJobs(e.target.value)}
          className="mt-1.5 w-24 rounded-md border border-neutral-700 bg-neutral-950 px-2.5 py-1.5 text-sm text-neutral-100"
        />
      </div>

      <div className="mt-4 border-t border-neutral-800 pt-4">
        <button
          type="button"
          disabled={!jobsValid || update.isPending}
          onClick={() =>
            update.mutate({
              bind_addr: bindAddr,
              docker_host: dockerHost,
              max_concurrent_jobs: jobsValue,
            })
          }
          className="rounded-md bg-accent px-3 py-1.5 text-sm font-medium text-white hover:bg-accent-dark disabled:opacity-50"
        >
          {update.isPending ? "Saving…" : "Save"}
        </button>
        {update.isError && <p className="mt-2 text-xs text-red-400">{(update.error as Error).message}</p>}
        {update.isSuccess && <p className="mt-2 text-xs text-neutral-500">Saved.</p>}
      </div>
    </div>
  );
}
