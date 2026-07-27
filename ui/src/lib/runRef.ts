export type RunRef = { kind: "pr"; number: number } | { kind: "branch"; name: string };

const PR_REF = /^refs\/pull\/(\d+)\/head$/;
const BRANCH_REF = /^refs\/heads\/(.+)$/;

function extractPrHeadRef(payloadJson: string): string | null {
  try {
    const payload = JSON.parse(payloadJson);
    const headRef = payload?.pull_request?.head?.ref;
    return typeof headRef === "string" ? headRef : null;
  } catch {
    return null;
  }
}

// A "manual" run (the dashboard's own "Run now" button) always targets the default branch, so a
// branch tag on it would just be noise. Every other trigger gets one when we can determine it: a
// push carries its own refs/heads/... ref, and a pull_request carries the head branch buried in
// the raw webhook payload since refs/pull/N/head isn't a real branch ref.
export function parseRunRefs(run: {
  trigger_event: string;
  ref_name: string | null;
  trigger_payload_json: string | null;
}): RunRef[] {
  if (run.trigger_event === "manual") return [];

  const prMatch = run.ref_name?.match(PR_REF);
  const branchMatch = run.ref_name?.match(BRANCH_REF);
  const refs: RunRef[] = [];

  if (branchMatch) {
    refs.push({ kind: "branch", name: branchMatch[1] });
  } else if (prMatch && run.trigger_payload_json) {
    const headRef = extractPrHeadRef(run.trigger_payload_json);
    if (headRef) refs.push({ kind: "branch", name: headRef });
  }

  if (prMatch) {
    refs.push({ kind: "pr", number: Number(prMatch[1]) });
  }

  return refs;
}

export function runRefGithubUrl(owner: string, name: string, ref: RunRef): string {
  return ref.kind === "pr"
    ? `https://github.com/${owner}/${name}/pull/${ref.number}`
    : `https://github.com/${owner}/${name}/tree/${encodeURIComponent(ref.name)}`;
}
