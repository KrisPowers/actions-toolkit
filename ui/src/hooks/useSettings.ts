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
