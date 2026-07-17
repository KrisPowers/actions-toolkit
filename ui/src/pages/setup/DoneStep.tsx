import { Check } from "lucide-react";

export default function DoneStep({ onFinish }: { onFinish: () => void }) {
  return (
    <div className="text-center">
      <div className="mx-auto flex h-10 w-10 items-center justify-center rounded-full bg-[var(--color-status-success)]/15 text-[var(--color-status-success)]">
        <Check className="h-5 w-5" strokeWidth={2.5} />
      </div>
      <h1 className="mt-4 text-lg font-semibold text-neutral-100">You are all set</h1>
      <p className="mt-2 text-sm text-neutral-400">
        Create workflows for your connected repos, or connect more from the Repos page at any time.
      </p>
      <button
        type="button"
        onClick={onFinish}
        className="mt-6 w-full rounded-md bg-accent px-3 py-2 text-sm font-medium text-white hover:bg-accent-hover"
      >
        Go to dashboard
      </button>
    </div>
  );
}
