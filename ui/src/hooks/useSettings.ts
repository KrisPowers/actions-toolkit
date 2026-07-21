import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { settingsApi } from "../api/settings";
import type { UpdateSettingsRequest } from "../api/types";

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

// Polls quickly while cloudflared is starting up (it usually takes a few seconds to print its
// assigned URL), then stops polling once the tunnel is running, failed, or was never started.
export function useCloudflareTunnelStatus() {
  return useQuery({
    queryKey: ["settings", "cloudflare-tunnel"],
    queryFn: settingsApi.cloudflareTunnelStatus,
    refetchInterval: (query) => (query.state.data?.status === "starting" ? 1000 : false),
  });
}

export function useStartCloudflareTunnel() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: settingsApi.startCloudflareTunnel,
    onSuccess: (data) => qc.setQueryData(["settings", "cloudflare-tunnel"], data),
  });
}

export function useStopCloudflareTunnel() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: settingsApi.stopCloudflareTunnel,
    onSuccess: (data) => qc.setQueryData(["settings", "cloudflare-tunnel"], data),
  });
}

// Polls quickly while `tailscale funnel` is starting up, then stops polling once the tunnel is
// running, failed, or was never started. Mirrors useCloudflareTunnelStatus above.
export function useTailscaleTunnelStatus() {
  return useQuery({
    queryKey: ["settings", "tailscale-tunnel"],
    queryFn: settingsApi.tailscaleTunnelStatus,
    refetchInterval: (query) => (query.state.data?.status === "starting" ? 1000 : false),
  });
}

export function useStartTailscaleTunnel() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: settingsApi.startTailscaleTunnel,
    onSuccess: (data) => qc.setQueryData(["settings", "tailscale-tunnel"], data),
  });
}

export function useStopTailscaleTunnel() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: settingsApi.stopTailscaleTunnel,
    onSuccess: (data) => qc.setQueryData(["settings", "tailscale-tunnel"], data),
  });
}

// Whether cloudflared/tailscale are actually installed, so the Webhooks page can disable each
// tunnel button up front instead of letting the operator click it and only then find out.
export function useTunnelAvailability() {
  return useQuery({ queryKey: ["settings", "tunnel-availability"], queryFn: settingsApi.tunnelAvailability });
}
