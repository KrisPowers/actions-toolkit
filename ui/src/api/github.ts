import { api } from "./client";

export const githubApi = {
  listIssues: (repoId: string, state: string = "open") => api.get<any[]>(`/repos/${repoId}/issues?state=${state}`),
  getIssue: (repoId: string, number: number) => api.get<any>(`/repos/${repoId}/issues/${number}`),
  addIssueComment: (repoId: string, number: number, body: string) =>
    api.post<any>(`/repos/${repoId}/issues/${number}/comments`, { body }),
  updateIssue: (repoId: string, number: number, patch: { state?: string; add_labels?: string[]; remove_label?: string }) =>
    api.patch<any>(`/repos/${repoId}/issues/${number}`, patch),

  listPullRequests: (repoId: string, state: string = "open") => api.get<any[]>(`/repos/${repoId}/pulls?state=${state}`),
  getPullRequest: (repoId: string, number: number) => api.get<any>(`/repos/${repoId}/pulls/${number}`),
  addPrComment: (repoId: string, number: number, body: string) =>
    api.post<any>(`/repos/${repoId}/pulls/${number}/comments`, { body }),

  listReleases: (repoId: string) => api.get<any[]>(`/repos/${repoId}/releases`),
  createRelease: (
    repoId: string,
    payload: { tag_name: string; name?: string; body?: string; draft?: boolean; prerelease?: boolean },
  ) => api.post<any>(`/repos/${repoId}/releases`, payload),
  updateRelease: (
    repoId: string,
    releaseId: number,
    patch: { name?: string; body?: string; draft?: boolean; prerelease?: boolean },
  ) => api.patch<any>(`/repos/${repoId}/releases/${releaseId}`, patch),
};
