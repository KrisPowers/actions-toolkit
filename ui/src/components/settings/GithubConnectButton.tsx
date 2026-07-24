import { useQueryClient } from "@tanstack/react-query";
import { ExternalLink } from "lucide-react";
import { githubAccountApi } from "../../api/githubAccount";
import GithubDeviceFlow from "../auth/GithubDeviceFlow";

interface ConnectedData {
  githubLogin: string;
  hasInstallation: boolean;
}

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
  presentation = "inline",
  onConnected,
}: {
  label?: string;
  variant?: "primary" | "outline";
  presentation?: "inline" | "modal";
  onConnected?: (hasInstallation: boolean) => void;
}) {
  const qc = useQueryClient();

  return (
    <GithubDeviceFlow<undefined, ConnectedData>
      label={label}
      variant={variant}
      presentation={presentation}
      start={async () => {
        const res = await githubAccountApi.deviceStart();
        return { pollKey: undefined, userCode: res.user_code, verificationUri: res.verification_uri, intervalSeconds: res.interval };
      }}
      poll={async () => {
        const res = await githubAccountApi.devicePoll();
        if (res.status === "pending" || res.status === "not_started") return { kind: "pending" };
        if (res.status === "denied") return { kind: "denied" };
        if (res.status === "expired") return { kind: "expired" };
        return { kind: "done", data: { githubLogin: res.github_login, hasInstallation: res.has_installation } };
      }}
      onDone={(data) => {
        qc.invalidateQueries({ queryKey: ["github", "token-status"] });
        qc.invalidateQueries({ queryKey: ["auth", "status"] });
        onConnected?.(data.hasInstallation);
      }}
    >
      {(data) => (
        <p className="text-sm text-[var(--color-status-success)]">
          Connected as @{data.githubLogin}.
          {!data.hasInstallation && (
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
      )}
    </GithubDeviceFlow>
  );
}
