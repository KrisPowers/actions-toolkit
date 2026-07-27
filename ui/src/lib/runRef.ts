export type RunRef = { kind: "pr"; number: number } | { kind: "branch"; name: string };

const PR_REF = /^refs\/pull\/(\d+)\/head$/;
const BRANCH_REF = /^refs\/heads\/(.+)$/;

export function parseRunRef(refName: string | null): RunRef | null {
  if (!refName) return null;
  const pr = refName.match(PR_REF);
  if (pr) return { kind: "pr", number: Number(pr[1]) };
  const branch = refName.match(BRANCH_REF);
  if (branch) return { kind: "branch", name: branch[1] };
  return null;
}

export function runRefGithubUrl(owner: string, name: string, ref: RunRef): string {
  return ref.kind === "pr"
    ? `https://github.com/${owner}/${name}/pull/${ref.number}`
    : `https://github.com/${owner}/${name}/tree/${encodeURIComponent(ref.name)}`;
}
