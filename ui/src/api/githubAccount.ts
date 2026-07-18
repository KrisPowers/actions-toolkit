import { api } from "./client";
import type { AccessibleRepo, GithubTokenStatus } from "./types";

export const githubAccountApi = {
  status: () => api.get<GithubTokenStatus>("/github/token"),
  deleteToken: () => api.delete<void>("/github/token"),
  accessibleRepos: () => api.get<AccessibleRepo[]>("/github/accessible-repos"),
};
