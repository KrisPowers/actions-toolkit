import { api } from "./client";
import type { AccessibleRepo, DevicePollResponse, DeviceStartResponse, GithubTokenStatus } from "./types";

export const githubAccountApi = {
  status: () => api.get<GithubTokenStatus>("/github/token"),
  deleteToken: () => api.delete<void>("/github/token"),
  accessibleRepos: () => api.get<AccessibleRepo[]>("/github/accessible-repos"),
  deviceStart: () => api.post<DeviceStartResponse>("/auth/github/device/start"),
  devicePoll: () => api.post<DevicePollResponse>("/auth/github/device/poll"),
};
