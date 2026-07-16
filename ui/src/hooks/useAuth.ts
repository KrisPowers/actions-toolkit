import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { authApi } from "../api/auth";

export function useAuthStatus() {
  return useQuery({ queryKey: ["auth", "status"], queryFn: authApi.status });
}

export function useMe() {
  return useQuery({ queryKey: ["auth", "me"], queryFn: authApi.me, retry: false });
}

export function useLogin() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ username, password }: { username: string; password: string }) => authApi.login(username, password),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["auth"] }),
  });
}

export function useSetup() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ username, password }: { username: string; password: string }) => authApi.setup(username, password),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["auth"] }),
  });
}

export function useLogout() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: authApi.logout,
    onSuccess: () => qc.invalidateQueries({ queryKey: ["auth"] }),
  });
}
