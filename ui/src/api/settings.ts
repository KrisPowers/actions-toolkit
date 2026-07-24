import { api } from "./client";
import type {
  DashboardTunnelRequest,
  NetworkInfo,
  RuntimeStatus,
  Settings,
  TunnelAvailability,
  TunnelProvider,
  TunnelState,
  UpdateSettingsRequest,
} from "./types";

export const settingsApi = {
  get: () => api.get<Settings>("/settings"),
  update: (patch: UpdateSettingsRequest) => api.patch<Settings>("/settings", patch),
  runtimeStatus: () => api.get<RuntimeStatus>("/settings/runtime-status"),
  networkInfo: () => api.get<NetworkInfo>("/settings/network-info"),
  tunnelAvailability: () => api.get<TunnelAvailability>("/settings/tunnel-availability"),

  // Dashboard/API remote-access tunnel: a singleton, entirely separate from any repo's webhook
  // tunnel (see repoTunnelApi below).
  dashboardTunnelStatus: () => api.get<TunnelState>("/settings/dashboard-tunnel"),
  startDashboardTunnel: (provider: TunnelProvider) => api.post<TunnelState>("/settings/dashboard-tunnel", { provider }),
  stopDashboardTunnel: () => api.delete<TunnelState>("/settings/dashboard-tunnel"),
  dashboardTunnelRequests: () => api.get<DashboardTunnelRequest[]>("/settings/dashboard-tunnel/requests"),
};
