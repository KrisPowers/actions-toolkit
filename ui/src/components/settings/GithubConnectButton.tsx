import GithubMark from "../common/GithubMark";

/**
 * A full browser navigation (not a fetch), since /api/auth/github/authorize is a redirect
 * flow: the browser has to actually land on GitHub's authorize page, not just receive a
 * redirect response a script would have to interpret itself.
 */
export default function GithubConnectButton({
  label = "Connect GitHub",
  variant = "primary",
}: {
  label?: string;
  variant?: "primary" | "outline";
}) {
  const className =
    variant === "primary"
      ? "inline-flex items-center gap-1.5 rounded-md bg-accent px-3 py-1.5 text-sm font-medium text-white hover:bg-accent-hover"
      : "inline-flex items-center gap-1.5 rounded-md border border-neutral-700 px-3 py-1.5 text-sm text-neutral-200 hover:bg-neutral-800";

  return (
    <a href="/api/auth/github/authorize" className={className}>
      <GithubMark className="h-4 w-4" />
      {label}
    </a>
  );
}
