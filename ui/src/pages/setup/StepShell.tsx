import type { ReactNode } from "react";

const STEP_LABELS = ["Welcome", "Admin account", "GitHub token", "Repos", "Done"];

export default function StepShell({ step, children }: { step: number; children: ReactNode }) {
  return (
    <div className="flex h-full w-full items-center justify-center p-4">
      <div className="w-full max-w-md">
        <div className="mb-6 flex items-center justify-center gap-2">
          {STEP_LABELS.map((label, i) => (
            <div key={label} className="flex items-center gap-2">
              <div
                className={`h-2 w-8 rounded-full transition-colors ${i <= step ? "bg-accent" : "bg-neutral-800"}`}
                title={label}
              />
            </div>
          ))}
        </div>
        <div className="rounded-lg border border-neutral-800 bg-neutral-900 p-6">{children}</div>
      </div>
    </div>
  );
}
