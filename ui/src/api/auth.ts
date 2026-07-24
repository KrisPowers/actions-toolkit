import { api } from "./client";
import type { LoginEvent, LoginPollResponse, LoginStartResponse, User, WhitelistEntry } from "./types";

export interface AuthStatus {
  needs_setup: boolean;
  needs_admin: boolean;
  needs_github_token: boolean;
}

export const authApi = {
  status: () => api.get<AuthStatus>("/auth/status"),
  loginStart: () => api.post<LoginStartResponse>("/auth/github/login/start"),
  loginPoll: (attemptId: string) => api.post<LoginPollResponse>("/auth/github/login/poll", { attempt_id: attemptId }),
  logout: () => api.post<void>("/auth/logout"),
  me: () => api.get<User>("/auth/me"),
  listUsers: () => api.get<User[]>("/users"),
  deleteUser: (id: string) => api.delete<void>(`/users/${id}`),
  setUserStatus: (id: string, status: string) => api.patch<void>(`/users/${id}/status`, { status }),
  setUserRole: (id: string, role: string) => api.patch<void>(`/users/${id}/role`, { role }),
  listWhitelist: () => api.get<WhitelistEntry[]>("/whitelist"),
  addWhitelist: (githubLogin: string) => api.post<void>("/whitelist", { github_login: githubLogin }),
  removeWhitelist: (githubLogin: string) => api.delete<void>(`/whitelist/${encodeURIComponent(githubLogin)}`),
  listLoginEvents: (limit = 50, offset = 0) => api.get<LoginEvent[]>(`/login-events?limit=${limit}&offset=${offset}`),
};
