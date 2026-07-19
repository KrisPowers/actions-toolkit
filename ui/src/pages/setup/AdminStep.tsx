import { useState } from "react";
import { useSetup } from "../../hooks/useAuth";
import Button from "../../components/common/Button";
import Input from "../../components/common/Input";

export default function AdminStep({ onNext }: { onNext: () => void }) {
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const setup = useSetup();

  function submit(e: React.FormEvent) {
    e.preventDefault();
    setup.mutate({ username, password }, { onSuccess: onNext });
  }

  return (
    <form onSubmit={submit}>
      <h1 className="text-lg font-semibold text-neutral-100">Create the admin account</h1>
      <p className="mt-1 text-sm text-neutral-400">This account manages workflows, repos, and other users.</p>

      <label className="mt-5 block text-xs font-medium text-neutral-400">Username</label>
      <Input value={username} onChange={(e) => setUsername(e.target.value)} className="mt-1 w-full" autoComplete="username" autoFocus />

      <label className="mt-4 block text-xs font-medium text-neutral-400">Password</label>
      <Input
        type="password"
        value={password}
        onChange={(e) => setPassword(e.target.value)}
        className="mt-1 w-full"
        autoComplete="new-password"
      />
      <p className="mt-1 text-xs text-neutral-600">At least 3 characters for the username, 8 for the password.</p>

      {setup.isError && <p className="mt-3 text-sm text-[var(--color-status-error)]">{(setup.error as Error).message}</p>}

      <Button type="submit" variant="primary" disabled={setup.isPending} className="mt-5 w-full">
        {setup.isPending ? "Creating…" : "Continue"}
      </Button>
    </form>
  );
}
