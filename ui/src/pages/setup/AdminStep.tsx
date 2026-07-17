import { useState } from "react";
import { useSetup } from "../../hooks/useAuth";

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
      <input
        value={username}
        onChange={(e) => setUsername(e.target.value)}
        className="mt-1 w-full rounded-md border border-neutral-700 bg-neutral-950 px-3 py-2 text-sm text-neutral-100 outline-none focus:border-accent"
        autoComplete="username"
        autoFocus
      />

      <label className="mt-4 block text-xs font-medium text-neutral-400">Password</label>
      <input
        type="password"
        value={password}
        onChange={(e) => setPassword(e.target.value)}
        className="mt-1 w-full rounded-md border border-neutral-700 bg-neutral-950 px-3 py-2 text-sm text-neutral-100 outline-none focus:border-accent"
        autoComplete="new-password"
      />
      <p className="mt-1 text-xs text-neutral-600">At least 3 characters for the username, 8 for the password.</p>

      {setup.isError && <p className="mt-3 text-sm text-[var(--color-status-error)]">{(setup.error as Error).message}</p>}

      <button
        type="submit"
        disabled={setup.isPending}
        className="mt-5 w-full rounded-md bg-accent px-3 py-2 text-sm font-medium text-white hover:bg-accent-hover disabled:opacity-60"
      >
        {setup.isPending ? "Creating…" : "Continue"}
      </button>
    </form>
  );
}
