import { useState } from "react";
import GithubConnectButton from "../../components/settings/GithubConnectButton";
import GithubMark from "../../components/common/GithubMark";
import Button from "../../components/common/Button";

export default function TokenStep({ onNext, onSkip }: { onNext: () => void; onSkip: () => void }) {
  // Authorizing (device flow) and installing the App on repos are two separate GitHub steps.
  // Advancing the wizard as soon as authorization completes let people skip installation
  // without ever seeing that it mattered, since API calls that need write access (like
  // updating a release) silently fail later without any App installation, even though
  // checkout works fine on the bare authorized token alone.
  const [needsInstallPrompt, setNeedsInstallPrompt] = useState(false);

  return (
    <div>
      <div className="flex items-center gap-2">
        <GithubMark className="h-5 w-5 text-neutral-400" />
        <h1 className="text-lg font-semibold text-neutral-100">Connect GitHub</h1>
      </div>
      <p className="mt-1 text-sm text-neutral-400">
        Connect your GitHub account to check out code, receive webhook events, and manage issues, PRs, and releases
        for every repo you connect.
      </p>

      <div className="mt-5">
        <GithubConnectButton
          onConnected={(hasInstallation) => {
            if (hasInstallation) onNext();
            else setNeedsInstallPrompt(true);
          }}
        />
      </div>

      {needsInstallPrompt && (
        <div className="mt-4">
          <Button variant="primary" className="w-full" onClick={onNext}>
            Continue
          </Button>
        </div>
      )}

      <button type="button" onClick={onSkip} className="mt-4 w-full rounded-md px-3 py-2 text-xs text-neutral-500 hover:text-neutral-300">
        Skip for now, add this later in Settings
      </button>
    </div>
  );
}
