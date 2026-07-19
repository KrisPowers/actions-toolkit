import { Check } from "lucide-react";
import Button from "../../components/common/Button";

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
      <Button variant="primary" onClick={onFinish} className="mt-6 w-full">
        Go to dashboard
      </Button>
    </div>
  );
}
