import { api } from "./client";
import type {
  CloudflareTunnelState,
  NetworkInfo,
  RuntimeStatus,
  Settings,
  TailscaleTunnelState,
  TunnelAvailability,
  UpdateSettingsRequest,
} from "./types";

export const settingsApi = {
  get: () => api.get<Settings>("/settings"),
  update: (patch: UpdateSettingsRequest) => api.patch<Settings>("/settings", patch),
  runtimeStatus: () => api.get<RuntimeStatus>("/settings/runtime-status"),
  networkInfo: () => api.get<NetworkInfo>("/settings/network-info"),
  cloudflareTunnelStatus: () => api.get<CloudflareTunnelState>("/settings/cloudflare-tunnel"),
  startCloudflareTunnel: () => api.post<CloudflareTunnelState>("/settings/cloudflare-tunnel"),
  stopCloudflareTunnel: () => api.delete<CloudflareTunnelState>("/settings/cloudflare-tunnel"),
  tailscaleTunnelStatus: () => api.get<TailscaleTunnelState>("/settings/tailscale-tunnel"),
  startTailscaleTunnel: () => api.post<TailscaleTunnelState>("/settings/tailscale-tunnel"),
  stopTailscaleTunnel: () => api.delete<TailscaleTunnelState>("/settings/tailscale-tunnel"),
  tunnelAvailability: () => api.get<TunnelAvailability>("/settings/tunnel-availability"),
};
