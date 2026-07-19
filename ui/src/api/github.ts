import { api } from "./client";
import type { GithubIssue, GithubPullRequest, GithubRelease } from "./types";

export const githubApi = {
  listIssues: (repoId: string, state: string = "open") => api.get<GithubIssue[]>(`/repos/${repoId}/issues?state=${state}`),
  getIssue: (repoId: string, number: number) => api.get<GithubIssue>(`/repos/${repoId}/issues/${number}`),
  addIssueComment: (repoId: string, number: number, body: string) =>
    api.post<GithubIssue>(`/repos/${repoId}/issues/${number}/comments`, { body }),
  updateIssue: (repoId: string, number: number, patch: { state?: string; add_labels?: string[]; remove_label?: string }) =>
    api.patch<GithubIssue>(`/repos/${repoId}/issues/${number}`, patch),

  listPullRequests: (repoId: string, state: string = "open") => api.get<GithubPullRequest[]>(`/repos/${repoId}/pulls?state=${state}`),
  getPullRequest: (repoId: string, number: number) => api.get<GithubPullRequest>(`/repos/${repoId}/pulls/${number}`),
  addPrComment: (repoId: string, number: number, body: string) =>
    api.post<GithubIssue>(`/repos/${repoId}/pulls/${number}/comments`, { body }),

  listReleases: (repoId: string) => api.get<GithubRelease[]>(`/repos/${repoId}/releases`),
  createRelease: (
    repoId: string,
    payload: { tag_name: string; name?: string; body?: string; draft?: boolean; prerelease?: boolean },
  ) => api.post<GithubRelease>(`/repos/${repoId}/releases`, payload),
  updateRelease: (
    repoId: string,
    releaseId: number,
    patch: { name?: string; body?: string; draft?: boolean; prerelease?: boolean },
  ) => api.patch<GithubRelease>(`/repos/${repoId}/releases/${releaseId}`, patch),
};
