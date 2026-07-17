import { useState } from "react";
import { useLogin } from "../hooks/useAuth";

export default function LoginPage() {
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const login = useLogin();

  function submit(e: React.FormEvent) {
    e.preventDefault();
    login.mutate({ username, password });
  }

  return (
    <div className="flex h-full w-full items-center justify-center">
      <form onSubmit={submit} className="w-full max-w-sm rounded-lg border border-neutral-800 bg-neutral-900 p-6">
        <div className="flex h-9 w-9 items-center justify-center rounded-md bg-accent text-sm font-bold text-white">A</div>
        <h1 className="mt-4 text-lg font-semibold text-neutral-100">Sign in</h1>
        <p className="mt-1 text-sm text-neutral-400">actions-toolkit</p>

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
          autoComplete="current-password"
        />

        {login.isError && <p className="mt-3 text-sm text-[var(--color-status-error)]">{(login.error as Error).message}</p>}

        <button
          type="submit"
          disabled={login.isPending}
          className="mt-5 w-full rounded-md bg-accent px-3 py-2 text-sm font-medium text-white hover:bg-accent-hover disabled:opacity-60"
        >
          {login.isPending ? "Signing in…" : "Sign in"}
        </button>
      </form>
    </div>
  );
}
