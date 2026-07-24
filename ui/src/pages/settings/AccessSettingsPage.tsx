import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Ban, CheckCircle2, ShieldCheck, UserPlus, Users, X } from "lucide-react";
import { authApi } from "../../api/auth";
import { useMe } from "../../hooks/useAuth";
import Avatar from "../../components/common/Avatar";
import Button from "../../components/common/Button";
import Input from "../../components/common/Input";
import Card from "../../components/common/Card";

export default function AccessSettingsPage() {
  const { data: me } = useMe();
  const isAdmin = me?.role === "admin";
  const qc = useQueryClient();

  const { data: users } = useQuery({ queryKey: ["users"], queryFn: authApi.listUsers });
  const { data: whitelist } = useQuery({ queryKey: ["whitelist"], queryFn: authApi.listWhitelist, enabled: isAdmin });

  const [newWhitelistLogin, setNewWhitelistLogin] = useState("");
  const invalidateUsers = () => qc.invalidateQueries({ queryKey: ["users"] });

  const setStatus = useMutation({
    mutationFn: ({ id, status }: { id: string; status: string }) => authApi.setUserStatus(id, status),
    onSuccess: invalidateUsers,
  });
  const setRole = useMutation({
    mutationFn: ({ id, role }: { id: string; role: string }) => authApi.setUserRole(id, role),
    onSuccess: invalidateUsers,
  });
  const deleteUser = useMutation({
    mutationFn: (id: string) => authApi.deleteUser(id),
    onSuccess: invalidateUsers,
  });
  const addWhitelist = useMutation({
    mutationFn: (login: string) => authApi.addWhitelist(login),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["whitelist"] });
      setNewWhitelistLogin("");
    },
  });
  const removeWhitelist = useMutation({
    mutationFn: (login: string) => authApi.removeWhitelist(login),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["whitelist"] }),
  });

  return (
    <div className="flex flex-col gap-5">
      <Card className="p-5">
        <div className="flex items-center gap-2">
          <Users className="h-4 w-4 text-neutral-500" strokeWidth={2} />
          <h2 className="text-sm font-semibold text-neutral-200">Users</h2>
        </div>

        <div className="mt-3 flex flex-col gap-2">
          {(users ?? []).map((u) => (
            <div key={u.id} className="flex items-center justify-between gap-3 rounded-md border border-neutral-800 px-3 py-2 text-sm">
              <div className="flex items-center gap-2.5">
                <Avatar login={u.github_login} src={u.avatar_url} size={24} />
                <div>
                  <div className="text-neutral-200">
                    @{u.github_login} <span className="text-xs text-neutral-500">({u.role})</span>
                  </div>
                  <div className="text-xs text-neutral-500 capitalize">{u.status}</div>
                </div>
              </div>

              {isAdmin && me?.id !== u.id && (
                <div className="flex items-center gap-1.5">
                  {u.status !== "approved" && (
                    <Button
                      variant="default"
                      size="sm"
                      onClick={() => setStatus.mutate({ id: u.id, status: "approved" })}
                      aria-label={`Approve @${u.github_login}`}
                    >
                      <CheckCircle2 className="h-3.5 w-3.5" strokeWidth={2} />
                      Approve
                    </Button>
                  )}
                  {u.status !== "restricted" && (
                    <Button
                      variant="default"
                      size="sm"
                      onClick={() => setStatus.mutate({ id: u.id, status: "restricted" })}
                      aria-label={`Restrict @${u.github_login}`}
                    >
                      <Ban className="h-3.5 w-3.5" strokeWidth={2} />
                      Restrict
                    </Button>
                  )}
                  <Button
                    variant="default"
                    size="sm"
                    onClick={() => setRole.mutate({ id: u.id, role: u.role === "admin" ? "member" : "admin" })}
                    aria-label={u.role === "admin" ? `Demote @${u.github_login} to member` : `Promote @${u.github_login} to admin`}
                  >
                    <ShieldCheck className="h-3.5 w-3.5" strokeWidth={2} />
                    {u.role === "admin" ? "Demote" : "Promote"}
                  </Button>
                  <Button
                    variant="invisible"
                    size="icon"
                    onClick={() => deleteUser.mutate(u.id)}
                    aria-label={`Remove @${u.github_login}`}
                    className="hover:text-[var(--color-status-error)]"
                  >
                    <X className="h-3.5 w-3.5" strokeWidth={2} />
                  </Button>
                </div>
              )}
            </div>
          ))}
          {users?.length === 0 && <p className="text-sm text-neutral-500">No users yet.</p>}
        </div>
      </Card>

      {isAdmin && (
        <Card className="p-5">
          <div className="flex items-center gap-2">
            <UserPlus className="h-4 w-4 text-neutral-500" strokeWidth={2} />
            <h2 className="text-sm font-semibold text-neutral-200">Whitelist</h2>
          </div>
          <p className="mt-1 text-xs text-neutral-500">A whitelisted GitHub login is auto-approved the first time they sign in.</p>

          <div className="mt-3 flex flex-wrap gap-2">
            <Input value={newWhitelistLogin} onChange={(e) => setNewWhitelistLogin(e.target.value)} placeholder="github-username" className="w-48" />
            <Button
              variant="primary"
              disabled={!newWhitelistLogin.trim() || addWhitelist.isPending}
              onClick={() => addWhitelist.mutate(newWhitelistLogin.trim())}
            >
              <UserPlus className="h-3.5 w-3.5" strokeWidth={2} />
              Add
            </Button>
          </div>

          <div className="mt-3 flex flex-col gap-2">
            {(whitelist ?? []).map((w) => (
              <div key={w.github_login} className="flex items-center justify-between rounded-md border border-neutral-800 px-3 py-2 text-sm">
                <span className="text-neutral-200">@{w.github_login}</span>
                <Button
                  variant="invisible"
                  size="icon"
                  onClick={() => removeWhitelist.mutate(w.github_login)}
                  aria-label={`Remove @${w.github_login} from the whitelist`}
                  className="hover:text-[var(--color-status-error)]"
                >
                  <X className="h-3.5 w-3.5" strokeWidth={2} />
                </Button>
              </div>
            ))}
            {whitelist?.length === 0 && <p className="text-sm text-neutral-500">No whitelist entries yet.</p>}
          </div>
        </Card>
      )}
    </div>
  );
}
