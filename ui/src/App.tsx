import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { useAuthStatus, useMe } from "./hooks/useAuth";
import SetupWizard from "./pages/setup/SetupWizard";
import LoginPage from "./pages/LoginPage";
import AccessPendingPage from "./pages/AccessPendingPage";
import AppShell from "./components/layout/AppShell";
import AppRoutes from "./routes";

export default function App() {
  const { data: status, isLoading: statusLoading } = useAuthStatus();
  const { data: me, isLoading: meLoading, isError: meError } = useMe();

  // Decided once from the first status read, then held locally: needs_setup flips to false
  // partway through the wizard (as soon as an admin + token exist), but the wizard still has
  // its repos/done steps left to show, so it must not be re-evaluated against live status.
  const [inWizard, setInWizard] = useState<boolean | null>(null);
  useEffect(() => {
    if (inWizard === null && status) {
      setInWizard(status.needs_setup);
    }
  }, [status, inWizard]);

  if (statusLoading || inWizard === null) {
    return <FullScreenMessage>Loading…</FullScreenMessage>;
  }

  if (inWizard) {
    return <SetupWizard initialStatus={status} onComplete={() => setInWizard(false)} />;
  }

  if (meLoading) {
    return <FullScreenMessage>Loading…</FullScreenMessage>;
  }

  if (meError || !me) {
    return <LoginPage />;
  }

  if (me.status !== "approved") {
    return <AccessPendingPage user={me} />;
  }

  return (
    <AppShell user={me}>
      <AppRoutes />
    </AppShell>
  );
}

function FullScreenMessage({ children }: { children: ReactNode }) {
  return <div className="flex h-full w-full items-center justify-center text-sm text-neutral-400">{children}</div>;
}
