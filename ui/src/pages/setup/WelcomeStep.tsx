export default function WelcomeStep({ onNext }: { onNext: () => void }) {
  return (
    <div>
      <div className="flex h-9 w-9 items-center justify-center rounded-md bg-accent text-sm font-bold text-white">A</div>
      <h1 className="mt-4 text-lg font-semibold text-neutral-100">Welcome to actions-toolkit</h1>
      <p className="mt-2 text-sm text-neutral-400">
        Run your CI/CD workflows on your own hardware instead of paying for GitHub-hosted runners. This
        setup takes about a minute: create an admin account, connect a GitHub token, and optionally pick
        the repos you want to run workflows for. Nothing is ever written to an env file, everything you
        enter here is stored encrypted in the local database.
      </p>
      <button
        type="button"
        onClick={onNext}
        className="mt-6 w-full rounded-md bg-accent px-3 py-2 text-sm font-medium text-white hover:bg-accent-dark"
      >
        Get started
      </button>
    </div>
  );
}
