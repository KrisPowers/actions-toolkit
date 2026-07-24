import { useEffect, useState } from "react";
import { ExternalLink, Loader2 } from "lucide-react";
import GithubMark from "../common/GithubMark";
import Button from "../common/Button";
import Modal from "../common/Modal";

export type DeviceFlowOutcome<TDone> = { kind: "pending" } | { kind: "denied" } | { kind: "expired" } | { kind: "done"; data: TDone };

type FlowState<K, TDone> =
  | { phase: "idle" }
  | { phase: "starting" }
  | { phase: "waiting"; pollKey: K; userCode: string; verificationUri: string; intervalSeconds: number }
  | { phase: "denied" }
  | { phase: "expired" }
  | { phase: "error"; message: string }
  | { phase: "done"; data: TDone };

/**
 * The shared GitHub device-flow UI: start a connect/login attempt, show the user code and
 * verification link, poll until GitHub reports a terminal outcome. Extracted from what was
 * originally `GithubConnectButton`'s own state machine so the same well-tested flow drives
 * both the account-wide repo-access connection and GitHub-based login, which differ only in
 * what "start" and "poll" mean against the API and what a successful outcome carries.
 */
export default function GithubDeviceFlow<K, TDone>({
  label = "Connect GitHub",
  variant = "primary",
  presentation = "inline",
  start,
  poll,
  onDone,
  children,
}: {
  label?: string;
  variant?: "primary" | "outline";
  /** "modal" shows the device-code flow as a popup instead of growing the space around the
   * button in place, for callers too height-constrained to absorb an inline block appearing
   * underneath it. */
  presentation?: "inline" | "modal";
  start: () => Promise<{ pollKey: K; userCode: string; verificationUri: string; intervalSeconds: number }>;
  poll: (pollKey: K) => Promise<DeviceFlowOutcome<TDone>>;
  onDone?: (data: TDone) => void;
  /** Rendered once the flow completes successfully, in place of the idle button. */
  children?: (data: TDone) => React.ReactNode;
}) {
  const [state, setState] = useState<FlowState<K, TDone>>({ phase: "idle" });

  useEffect(() => {
    if (state.phase !== "waiting") return;
    const { pollKey, intervalSeconds } = state;

    let cancelled = false;
    const timer = setInterval(async () => {
      try {
        const outcome = await poll(pollKey);
        if (cancelled) return;
        if (outcome.kind === "pending") return;
        if (outcome.kind === "denied") setState({ phase: "denied" });
        else if (outcome.kind === "expired") setState({ phase: "expired" });
        else {
          setState({ phase: "done", data: outcome.data });
          onDone?.(outcome.data);
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

  async function begin() {
    setState({ phase: "starting" });
    try {
      const res = await start();
      setState({ phase: "waiting", pollKey: res.pollKey, userCode: res.userCode, verificationUri: res.verificationUri, intervalSeconds: res.intervalSeconds });
    } catch (e) {
      setState({ phase: "error", message: (e as Error).message });
    }
  }

  if (state.phase === "idle" || state.phase === "starting") {
    return (
      <Button variant={variant === "primary" ? "primary" : "default"} onClick={begin} disabled={state.phase === "starting"}>
        <GithubMark className="h-4 w-4" />
        {state.phase === "starting" ? "Starting…" : label}
      </Button>
    );
  }

  if (state.phase === "done") {
    return <>{children?.(state.data)}</>;
  }

  const flow = (
    <div className={presentation === "inline" ? "rounded-md border border-neutral-700 bg-neutral-950 p-3 text-sm" : "text-sm"}>
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
          <Button variant="invisible" size="sm" onClick={() => setState({ phase: "idle" })} className="mt-2 text-xs">
            Cancel
          </Button>
        </>
      )}
      {state.phase === "denied" && (
        <p className="text-[var(--color-status-error)]">
          Authorization was denied on GitHub.{" "}
          <button type="button" onClick={begin} className="underline">
            Try again
          </button>
        </p>
      )}
      {state.phase === "expired" && (
        <p className="text-[var(--color-status-error)]">
          That code expired before it was used.{" "}
          <button type="button" onClick={begin} className="underline">
            Try again
          </button>
        </p>
      )}
      {state.phase === "error" && (
        <p className="text-[var(--color-status-error)]">
          {state.message}{" "}
          <button type="button" onClick={begin} className="underline">
            Try again
          </button>
        </p>
      )}
    </div>
  );

  if (presentation === "modal") {
    return (
      <Modal open onClose={() => setState({ phase: "idle" })} className="max-w-sm">
        {flow}
      </Modal>
    );
  }

  return flow;
}
