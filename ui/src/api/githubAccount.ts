import { api } from "./client";
import type { AccessibleRepo, GithubTokenStatus } from "./types";

export const githubAccountApi = {
  status: () => api.get<GithubTokenStatus>("/github/token"),
  setToken: (token: string) => api.post<GithubTokenStatus>("/github/token", { token }),
  deleteToken: () => api.delete<void>("/github/token"),
  accessibleRepos: () => api.get<AccessibleRepo[]>("/github/accessible-repos"),
};
