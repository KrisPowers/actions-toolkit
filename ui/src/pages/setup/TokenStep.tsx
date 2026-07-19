import GithubConnectButton from "../../components/settings/GithubConnectButton";
import GithubMark from "../../components/common/GithubMark";

export default function TokenStep({ onNext, onSkip }: { onNext: () => void; onSkip: () => void }) {
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
        <GithubConnectButton onConnected={onNext} />
      </div>

      <button type="button" onClick={onSkip} className="mt-4 w-full rounded-md px-3 py-2 text-xs text-neutral-500 hover:text-neutral-300">
        Skip for now, add this later in Settings
      </button>
    </div>
  );
}
