import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
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
    <div className="max-w-lg">
      <h1 className="text-lg font-semibold text-neutral-100">Settings</h1>

      <div className="mt-5">
        <GithubConnectionCard />
      </div>

      <div className="mt-5">
        <RuntimeSettingsCard />
      </div>

      <div className="mt-5 rounded-lg border border-neutral-800 bg-neutral-900 p-5">
        <h2 className="text-sm font-semibold text-neutral-200">Users</h2>
        <div className="mt-3 divide-y divide-neutral-800">
          {(users ?? []).map((u) => (
            <div key={u.id} className="flex items-center justify-between py-2 text-sm">
              <span className="text-neutral-200">
                {u.username} <span className="text-xs text-neutral-500">({u.role})</span>
              </span>
              {me?.id !== u.id && (
                <button
                  type="button"
                  onClick={() => deleteUser.mutate(u.id)}
                  className="text-xs text-red-400 hover:underline"
                >
                  Remove
                </button>
              )}
            </div>
          ))}
        </div>

        {me?.role === "admin" && (
          <div className="mt-4 border-t border-neutral-800 pt-4">
            <div className="text-xs font-medium text-neutral-400">Add a user</div>
            <div className="mt-2 flex gap-2">
              <input
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                placeholder="username"
                className="w-32 rounded-md border border-neutral-700 bg-neutral-950 px-2.5 py-1.5 text-sm text-neutral-100"
              />
              <input
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder="password"
                className="w-32 rounded-md border border-neutral-700 bg-neutral-950 px-2.5 py-1.5 text-sm text-neutral-100"
              />
              <button
                type="button"
                disabled={!username || !password || createUser.isPending}
                onClick={() => createUser.mutate()}
                className="rounded-md bg-accent px-3 py-1.5 text-sm font-medium text-white hover:bg-accent-dark disabled:opacity-50"
              >
                Add
              </button>
            </div>
            {createUser.isError && <p className="mt-2 text-xs text-red-400">{(createUser.error as Error).message}</p>}
          </div>
        )}
      </div>
    </div>
  );
}
