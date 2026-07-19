import GithubMark from "../common/GithubMark";
import { buttonClass } from "../common/Button";

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
  return (
    <a href="/api/auth/github/authorize" className={buttonClass(variant === "primary" ? "primary" : "default")}>
      <GithubMark className="h-4 w-4" />
      {label}
    </a>
  );
}
