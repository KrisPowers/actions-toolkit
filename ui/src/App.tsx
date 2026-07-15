import type { ReactNode } from "react";
import { useAuthStatus, useMe } from "./hooks/useAuth";
import SetupPage from "./pages/SetupPage";
import LoginPage from "./pages/LoginPage";
import AppShell from "./components/layout/AppShell";
import AppRoutes from "./routes";

export default function App() {
  const { data: status, isLoading: statusLoading } = useAuthStatus();
  const { data: me, isLoading: meLoading, isError: meError } = useMe();

  if (statusLoading) {
    return <FullScreenMessage>Loading…</FullScreenMessage>;
  }

  if (status?.needs_setup) {
    return <SetupPage />;
  }

  if (meLoading) {
    return <FullScreenMessage>Loading…</FullScreenMessage>;
  }

  if (meError || !me) {
    return <LoginPage />;
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
