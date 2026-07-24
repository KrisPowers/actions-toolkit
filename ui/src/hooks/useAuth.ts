import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { authApi } from "../api/auth";

export function useAuthStatus() {
  return useQuery({ queryKey: ["auth", "status"], queryFn: authApi.status });
}

export function useMe() {
  return useQuery({ queryKey: ["auth", "me"], queryFn: authApi.me, retry: false });
}

export function useLoginStart() {
  return useMutation({ mutationFn: authApi.loginStart });
}

export function useLoginPoll() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (attemptId: string) => authApi.loginPoll(attemptId),
    onSuccess: (res) => {
      if (res.status === "approved" || res.status === "pending_approval" || res.status === "restricted") {
        qc.invalidateQueries({ queryKey: ["auth"] });
      }
    },
  });
}

export function useLogout() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: authApi.logout,
    onSuccess: () => qc.invalidateQueries({ queryKey: ["auth"] }),
  });
}
