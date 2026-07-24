import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { repoTunnelApi, settingsApi } from "../api/settings";
import type { TunnelProvider, UpdateSettingsRequest } from "../api/types";

export function useSettings() {
  return useQuery({ queryKey: ["settings"], queryFn: settingsApi.get });
}

export function useUpdateSettings() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (patch: UpdateSettingsRequest) => settingsApi.update(patch),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["settings"] }),
  });
}

// Polls so that starting (or stopping) the Docker daemon while a page is open is picked up
// without the user needing to reload.
export function useRuntimeStatus() {
  return useQuery({ queryKey: ["settings", "runtime-status"], queryFn: settingsApi.runtimeStatus, refetchInterval: 5000 });
}

export function useNetworkInfo() {
  return useQuery({ queryKey: ["settings", "network-info"], queryFn: settingsApi.networkInfo });
}

// Whether cloudflared/tailscale are actually installed, so a tunnel option can be disabled up
// front instead of letting the operator pick it and only then find out.
export function useTunnelAvailability() {
  return useQuery({ queryKey: ["settings", "tunnel-availability"], queryFn: settingsApi.tunnelAvailability });
}

// Each repo's webhook tunnel is tracked independently on the backend (its own process, its own
// listener); these hooks are keyed by repoId so one repo's status/mutations never touch
// another's cached data.

export function useRepoTunnelStatus(repoId: string | undefined) {
  return useQuery({
    queryKey: ["repo-tunnel", repoId],
    queryFn: () => repoTunnelApi.status(repoId!),
    enabled: !!repoId,
    refetchInterval: (query) => (query.state.data?.status === "starting" ? 1000 : false),
  });
}

export function useStartRepoTunnel(repoId: string | undefined) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (provider: TunnelProvider) => repoTunnelApi.start(repoId!, provider),
    onSuccess: (data) => qc.setQueryData(["repo-tunnel", repoId], data),
  });
}

export function useStopRepoTunnel(repoId: string | undefined) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => repoTunnelApi.stop(repoId!),
    onSuccess: (data) => qc.setQueryData(["repo-tunnel", repoId], data),
  });
}

// The dashboard/API remote-access tunnel is a singleton, entirely separate from any repo's
// webhook tunnel above.

export function useDashboardTunnelStatus() {
  return useQuery({
    queryKey: ["settings", "dashboard-tunnel"],
    queryFn: settingsApi.dashboardTunnelStatus,
    refetchInterval: (query) => (query.state.data?.status === "starting" ? 1000 : false),
  });
}

export function useStartDashboardTunnel() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (provider: TunnelProvider) => settingsApi.startDashboardTunnel(provider),
    onSuccess: (data) => qc.setQueryData(["settings", "dashboard-tunnel"], data),
  });
}

export function useStopDashboardTunnel() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: settingsApi.stopDashboardTunnel,
    onSuccess: (data) => qc.setQueryData(["settings", "dashboard-tunnel"], data),
  });
}
