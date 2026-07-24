import { api } from "./client";
import type { NetworkInfo, RuntimeStatus, Settings, TunnelAvailability, UpdateSettingsRequest } from "./types";

export const settingsApi = {
  get: () => api.get<Settings>("/settings"),
  update: (patch: UpdateSettingsRequest) => api.patch<Settings>("/settings", patch),
  runtimeStatus: () => api.get<RuntimeStatus>("/settings/runtime-status"),
  networkInfo: () => api.get<NetworkInfo>("/settings/network-info"),
  tunnelAvailability: () => api.get<TunnelAvailability>("/settings/tunnel-availability"),
};
