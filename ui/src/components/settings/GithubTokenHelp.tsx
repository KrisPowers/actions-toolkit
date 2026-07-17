import { ExternalLink } from "lucide-react";
import InfoPopover from "../common/InfoPopover";

const STEPS = [
  "Go to github.com and sign in.",
  "Click your profile picture in the top-right corner, then select Settings.",
  "In the left sidebar, scroll down and click Developer settings.",
  "Click Personal access tokens, then Tokens (classic).",
  "Click Generate new token, then Generate new token (classic).",
  "Name it (e.g. \"actions-toolkit\"), set an expiration, and check the repo scope.",
  "Click Generate token at the bottom of the page.",
  "Copy the token now — GitHub only shows it once — and paste it in the field here.",
];

export default function GithubTokenHelp({ align = "left" }: { align?: "left" | "right" }) {
  return (
    <InfoPopover label="How to get a GitHub personal access token" align={align}>
      <p className="font-medium text-neutral-100">Get a personal access token</p>
      <ol className="mt-2 list-decimal space-y-1.5 pl-4 text-neutral-400">
        {STEPS.map((step) => (
          <li key={step}>{step}</li>
        ))}
      </ol>
      <a
        href="https://github.com/settings/tokens/new"
        target="_blank"
        rel="noreferrer"
        className="mt-3 inline-flex items-center gap-1.5 text-accent hover:underline"
      >
        Open GitHub token settings
        <ExternalLink className="h-3.5 w-3.5" strokeWidth={2} />
      </a>
    </InfoPopover>
  );
}
