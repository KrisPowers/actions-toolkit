import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { UserPlus, Users, X } from "lucide-react";
import { authApi } from "../api/auth";
import { useMe } from "../hooks/useAuth";
import GithubConnectionCard from "../components/settings/GithubConnectionCard";
import RuntimeSettingsCard from "../components/settings/RuntimeSettingsCard";

export default function SettingsPage() {
  const { data: me } = useMe();
  const qc = useQueryClient();
  const { data: users } = useQuery({ queryKey: ["users"], queryFn: authApi.listUsers });
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");

  const createUser = useMutation({
    mutationFn: () => authApi.createUser(username, password),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["users"] });
      setUsername("");
      setPassword("");
    },
  });

  const deleteUser = useMutation({
    mutationFn: (id: string) => authApi.deleteUser(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["users"] }),
  });

  return (
    <div className="max-w-6xl">
      <h1 className="text-lg font-semibold text-neutral-100">Settings</h1>

      <div className="mt-5 grid grid-cols-1 gap-5 xl:grid-cols-2">
        <GithubConnectionCard />
        <RuntimeSettingsCard />
      </div>

      <div className="mt-5 rounded-lg border border-neutral-800 bg-neutral-900 p-5">
        <div className="flex items-center gap-2">
          <Users className="h-4 w-4 text-neutral-500" strokeWidth={2} />
          <h2 className="text-sm font-semibold text-neutral-200">Users</h2>
        </div>

        <div className="mt-3 grid grid-cols-1 gap-2 sm:grid-cols-2 lg:grid-cols-3">
          {(users ?? []).map((u) => (
            <div key={u.id} className="flex items-center justify-between rounded-md border border-neutral-800 px-3 py-2 text-sm">
              <span className="text-neutral-200">
                {u.username} <span className="text-xs text-neutral-500">({u.role})</span>
              </span>
              {me?.id !== u.id && (
                <button
                  type="button"
                  onClick={() => deleteUser.mutate(u.id)}
                  aria-label={`Remove ${u.username}`}
                  className="text-neutral-500 hover:text-[var(--color-status-error)]"
                >
                  <X className="h-3.5 w-3.5" strokeWidth={2} />
                </button>
              )}
            </div>
          ))}
        </div>

        {me?.role === "admin" && (
          <div className="mt-4 border-t border-neutral-800 pt-4">
            <div className="text-xs font-medium text-neutral-400">Add a user</div>
            <div className="mt-2 flex flex-wrap gap-2">
              <input
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                placeholder="username"
                className="w-40 rounded-md border border-neutral-700 bg-neutral-950 px-2.5 py-1.5 text-sm text-neutral-100 outline-none focus:border-accent"
              />
              <input
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder="password"
                className="w-40 rounded-md border border-neutral-700 bg-neutral-950 px-2.5 py-1.5 text-sm text-neutral-100 outline-none focus:border-accent"
              />
              <button
                type="button"
                disabled={!username || !password || createUser.isPending}
                onClick={() => createUser.mutate()}
                className="inline-flex items-center gap-1.5 rounded-md bg-accent px-3 py-1.5 text-sm font-medium text-white hover:bg-accent-hover disabled:opacity-50"
              >
                <UserPlus className="h-3.5 w-3.5" strokeWidth={2} />
                Add
              </button>
            </div>
            {createUser.isError && <p className="mt-2 text-xs text-[var(--color-status-error)]">{(createUser.error as Error).message}</p>}
          </div>
        )}
      </div>
    </div>
  );
}
