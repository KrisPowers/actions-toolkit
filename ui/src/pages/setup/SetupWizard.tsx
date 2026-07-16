import { useState } from "react";
import type { AuthStatus } from "../../api/auth";
import StepShell from "./StepShell";
import WelcomeStep from "./WelcomeStep";
import AdminStep from "./AdminStep";
import TokenStep from "./TokenStep";
import ReposStep from "./ReposStep";
import DoneStep from "./DoneStep";

type Step = "welcome" | "admin" | "token" | "repos" | "done";

const STEP_INDEX: Record<Step, number> = { welcome: 0, admin: 1, token: 2, repos: 3, done: 4 };

/**
 * A resumed setup (server restarted mid-wizard on a prior visit) skips straight to whichever
 * step is still outstanding rather than re-running steps that are already done.
 */
function initialStep(status: AuthStatus | undefined): Step {
  if (!status || status.needs_admin) return "welcome";
  if (status.needs_github_token) return "token";
  return "repos";
}

export default function SetupWizard({ initialStatus, onComplete }: { initialStatus?: AuthStatus; onComplete: () => void }) {
  const [step, setStep] = useState<Step>(() => initialStep(initialStatus));

  return (
    <StepShell step={STEP_INDEX[step]}>
      {step === "welcome" && <WelcomeStep onNext={() => setStep("admin")} />}
      {step === "admin" && <AdminStep onNext={() => setStep("token")} />}
      {step === "token" && <TokenStep onNext={() => setStep("repos")} onSkip={() => setStep("repos")} />}
      {step === "repos" && <ReposStep onNext={() => setStep("done")} />}
      {step === "done" && <DoneStep onFinish={onComplete} />}
    </StepShell>
  );
}
