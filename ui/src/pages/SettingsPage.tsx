import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { UserPlus, Users, X } from "lucide-react";
import { authApi } from "../api/auth";
import { useMe } from "../hooks/useAuth";
import GithubConnectionCard from "../components/settings/GithubConnectionCard";
import RuntimeSettingsCard from "../components/settings/RuntimeSettingsCard";
import Button from "../components/common/Button";
import Input from "../components/common/Input";
import Card from "../components/common/Card";
import PageHeader from "../components/common/PageHeader";

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
      <PageHeader title="Settings" />

      <div className="mt-5 grid grid-cols-1 gap-5 xl:grid-cols-2">
        <GithubConnectionCard />
        <RuntimeSettingsCard />
      </div>

      <Card className="mt-5 p-5">
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
                <Button
                  variant="invisible"
                  size="icon"
                  onClick={() => deleteUser.mutate(u.id)}
                  aria-label={`Remove ${u.username}`}
                  className="hover:text-[var(--color-status-error)]"
                >
                  <X className="h-3.5 w-3.5" strokeWidth={2} />
                </Button>
              )}
            </div>
          ))}
        </div>

        {me?.role === "admin" && (
          <div className="mt-4 border-t border-neutral-800 pt-4">
            <div className="text-xs font-medium text-neutral-400">Add a user</div>
            <div className="mt-2 flex flex-wrap gap-2">
              <Input value={username} onChange={(e) => setUsername(e.target.value)} placeholder="username" className="w-40" />
              <Input type="password" value={password} onChange={(e) => setPassword(e.target.value)} placeholder="password" className="w-40" />
              <Button variant="primary" disabled={!username || !password || createUser.isPending} onClick={() => createUser.mutate()}>
                <UserPlus className="h-3.5 w-3.5" strokeWidth={2} />
                Add
              </Button>
            </div>
            {createUser.isError && <p className="mt-2 text-xs text-[var(--color-status-error)]">{(createUser.error as Error).message}</p>}
          </div>
        )}
      </Card>
    </div>
  );
}
