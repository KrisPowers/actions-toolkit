import { useState } from "react";
import { useLogin } from "../hooks/useAuth";
import Button from "../components/common/Button";
import Input from "../components/common/Input";
import BrandMark from "../components/common/BrandMark";
import { cardClass } from "../components/common/Card";

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
      <form onSubmit={submit} className={cardClass("w-full max-w-sm p-6")}>
        <BrandMark size={36} />
        <h1 className="mt-4 text-lg font-semibold text-neutral-100">Sign in</h1>
        <p className="mt-1 text-sm text-neutral-400">actions-toolkit</p>

        <label className="mt-5 block text-xs font-medium text-neutral-400">Username</label>
        <Input value={username} onChange={(e) => setUsername(e.target.value)} className="mt-1 w-full" autoComplete="username" />

        <label className="mt-4 block text-xs font-medium text-neutral-400">Password</label>
        <Input
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          className="mt-1 w-full"
          autoComplete="current-password"
        />

        {login.isError && <p className="mt-3 text-sm text-[var(--color-status-error)]">{(login.error as Error).message}</p>}

        <Button type="submit" variant="primary" disabled={login.isPending} className="mt-5 w-full">
          {login.isPending ? "Signing in…" : "Sign in"}
        </Button>
      </form>
    </div>
  );
}
