import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { ExternalLink, Loader2 } from "lucide-react";
import GithubMark from "../common/GithubMark";
import { githubAccountApi } from "../../api/githubAccount";

type FlowState =
  | { phase: "idle" }
  | { phase: "starting" }
  | { phase: "waiting"; userCode: string; verificationUri: string; intervalSeconds: number }
  | { phase: "denied" }
  | { phase: "expired" }
  | { phase: "error"; message: string }
  | { phase: "connected"; githubLogin: string; hasInstallation: boolean };

/**
 * GitHub requires a client_secret for the redirect-based authorization-code flow even with PKCE
 * (confirmed against GitHub's own docs, not an assumption), which a distributed binary with no
 * central backend can't hold safely. Device flow is the one GitHub OAuth flow that genuinely
 * needs no secret, so "Connect" here starts it, shows the code GitHub wants the operator to enter
 * at a separate URL, and polls until they've done that (or declined, or the code expired).
 */
export default function GithubConnectButton({
  label = "Connect GitHub",
  variant = "primary",
  onConnected,
}: {
  label?: string;
  variant?: "primary" | "outline";
  onConnected?: () => void;
}) {
  const [state, setState] = useState<FlowState>({ phase: "idle" });
  const qc = useQueryClient();

  useEffect(() => {
    if (state.phase !== "waiting") return;
    const { intervalSeconds } = state;

    let cancelled = false;
    const timer = setInterval(async () => {
      try {
        const res = await githubAccountApi.devicePoll();
        if (cancelled) return;
        if (res.status === "pending" || res.status === "not_started") return;
        if (res.status === "denied") setState({ phase: "denied" });
        else if (res.status === "expired") setState({ phase: "expired" });
        else if (res.status === "connected") {
          setState({ phase: "connected", githubLogin: res.github_login, hasInstallation: res.has_installation });
          qc.invalidateQueries({ queryKey: ["github", "token-status"] });
          qc.invalidateQueries({ queryKey: ["auth", "status"] });
          onConnected?.();
        }
      } catch {
        if (!cancelled) setState({ phase: "error", message: "Lost contact with the server while waiting on GitHub." });
      }
    }, intervalSeconds * 1000);

    return () => {
      cancelled = true;
      clearInterval(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [state.phase === "waiting" ? state.intervalSeconds : null]);

  async function start() {
    setState({ phase: "starting" });
    try {
      const res = await githubAccountApi.deviceStart();
      setState({ phase: "waiting", userCode: res.user_code, verificationUri: res.verification_uri, intervalSeconds: res.interval });
    } catch (e) {
      setState({ phase: "error", message: (e as Error).message });
    }
  }

  const buttonClassName =
    variant === "primary"
      ? "inline-flex items-center gap-1.5 rounded-md bg-accent px-3 py-1.5 text-sm font-medium text-white hover:bg-accent-hover disabled:opacity-60"
      : "inline-flex items-center gap-1.5 rounded-md border border-neutral-700 px-3 py-1.5 text-sm text-neutral-200 hover:bg-neutral-800 disabled:opacity-60";

  if (state.phase === "idle" || state.phase === "starting") {
    return (
      <button type="button" onClick={start} disabled={state.phase === "starting"} className={buttonClassName}>
        <GithubMark className="h-4 w-4" />
        {state.phase === "starting" ? "Starting…" : label}
      </button>
    );
  }

  if (state.phase === "connected") {
    return (
      <p className="text-sm text-[var(--color-status-success)]">
        Connected as @{state.githubLogin}.
        {!state.hasInstallation && (
          <>
            {" "}
            Install the App on your repos to finish:{" "}
            <a
              href="https://github.com/apps/actionstoolkit/installations/new"
              target="_blank"
              rel="noreferrer"
              className="inline-flex items-center gap-1 text-accent hover:underline"
            >
              github.com/apps/actionstoolkit
              <ExternalLink className="h-3 w-3" strokeWidth={2} />
            </a>
          </>
        )}
      </p>
    );
  }

  return (
    <div className="rounded-md border border-neutral-700 bg-neutral-950 p-3 text-sm">
      {state.phase === "waiting" && (
        <>
          <p className="flex items-center gap-1.5 text-neutral-300">
            <Loader2 className="h-3.5 w-3.5 animate-spin" strokeWidth={2} />
            Waiting for you to authorize on GitHub…
          </p>
          <p className="mt-2 text-neutral-400">
            1. Open{" "}
            <a
              href={state.verificationUri}
              target="_blank"
              rel="noreferrer"
              className="inline-flex items-center gap-1 text-accent hover:underline"
            >
              {state.verificationUri}
              <ExternalLink className="h-3 w-3" strokeWidth={2} />
            </a>
          </p>
          <p className="mt-1 text-neutral-400">2. Enter this code:</p>
          <code className="mt-1 block rounded bg-neutral-900 px-2 py-1.5 text-center text-base font-semibold tracking-widest text-neutral-100">
            {state.userCode}
          </code>
          <button type="button" onClick={() => setState({ phase: "idle" })} className="mt-2 text-xs text-neutral-500 hover:text-neutral-300">
            Cancel
          </button>
        </>
      )}
      {state.phase === "denied" && (
        <p className="text-[var(--color-status-error)]">
          Authorization was denied on GitHub.{" "}
          <button type="button" onClick={start} className="underline">
            Try again
          </button>
        </p>
      )}
      {state.phase === "expired" && (
        <p className="text-[var(--color-status-error)]">
          That code expired before it was used.{" "}
          <button type="button" onClick={start} className="underline">
            Try again
          </button>
        </p>
      )}
      {state.phase === "error" && (
        <p className="text-[var(--color-status-error)]">
          {state.message}{" "}
          <button type="button" onClick={start} className="underline">
            Try again
          </button>
        </p>
      )}
    </div>
  );
}
