import { useState } from "react";
import { useSetup } from "../hooks/useAuth";

export default function SetupPage() {
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const setup = useSetup();

  function submit(e: React.FormEvent) {
    e.preventDefault();
    setup.mutate({ username, password });
  }

  return (
    <div className="flex h-full w-full items-center justify-center">
      <form onSubmit={submit} className="w-full max-w-sm rounded-lg border border-neutral-800 bg-neutral-900 p-6">
        <h1 className="text-lg font-semibold text-neutral-100">Create the admin account</h1>
        <p className="mt-1 text-sm text-neutral-400">This is the first run of actions-toolkit. Create the initial administrator.</p>

        <label className="mt-5 block text-xs font-medium text-neutral-400">Username</label>
        <input
          value={username}
          onChange={(e) => setUsername(e.target.value)}
          className="mt-1 w-full rounded-md border border-neutral-700 bg-neutral-950 px-3 py-2 text-sm text-neutral-100 outline-none focus:border-accent"
          autoComplete="username"
        />

        <label className="mt-4 block text-xs font-medium text-neutral-400">Password</label>
        <input
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          className="mt-1 w-full rounded-md border border-neutral-700 bg-neutral-950 px-3 py-2 text-sm text-neutral-100 outline-none focus:border-accent"
          autoComplete="new-password"
        />

        {setup.isError && <p className="mt-3 text-sm text-red-400">{(setup.error as Error).message}</p>}

        <button
          type="submit"
          disabled={setup.isPending}
          className="mt-5 w-full rounded-md bg-accent px-3 py-2 text-sm font-medium text-white hover:bg-accent-dark disabled:opacity-60"
        >
          {setup.isPending ? "Creating…" : "Create admin account"}
        </button>
      </form>
    </div>
  );
}
