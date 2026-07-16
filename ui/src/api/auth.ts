import { api } from "./client";
import type { User } from "./types";

export interface AuthStatus {
  needs_setup: boolean;
  needs_admin: boolean;
  needs_github_token: boolean;
}

export const authApi = {
  status: () => api.get<AuthStatus>("/auth/status"),
  setup: (username: string, password: string) => api.post<User>("/auth/setup", { username, password }),
  login: (username: string, password: string) => api.post<User>("/auth/login", { username, password }),
  logout: () => api.post<void>("/auth/logout"),
  me: () => api.get<User>("/auth/me"),
  listUsers: () => api.get<User[]>("/users"),
  createUser: (username: string, password: string, role?: string) =>
    api.post<User>("/users", { username, password, role }),
  deleteUser: (id: string) => api.delete<void>(`/users/${id}`),
};
