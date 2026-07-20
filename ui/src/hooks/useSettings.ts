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
