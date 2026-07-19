import Button from "../../components/common/Button";

export default function WelcomeStep({ onNext }: { onNext: () => void }) {
  return (
    <div>
      <div className="flex h-9 w-9 items-center justify-center rounded-md bg-accent text-sm font-bold text-white">A</div>
      <h1 className="mt-4 text-lg font-semibold text-neutral-100">Welcome to actions-toolkit</h1>
      <p className="mt-2 text-sm text-neutral-400">
        Run CI/CD workflows on your own hardware. This takes about a minute: create an admin account, connect
        GitHub, and pick the repos to run workflows for. Everything you enter is stored encrypted, never in an env
        file.
      </p>
      <Button variant="primary" onClick={onNext} className="mt-6 w-full">
        Get started
      </Button>
    </div>
  );
}
