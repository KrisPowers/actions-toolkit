import { Navigate, Route, Routes } from "react-router-dom";
import DashboardPage from "./pages/DashboardPage";
import AnalyticsPage from "./pages/AnalyticsPage";
import RepoConnectPage from "./pages/RepoConnectPage";
import RepoSettingsPage from "./pages/RepoSettingsPage";
import WorkflowListPage from "./pages/WorkflowListPage";
import WorkflowEditorPage from "./pages/WorkflowEditorPage";
import RunListPage from "./pages/RunListPage";
import RunDetailPage from "./pages/RunDetailPage";
import ArtifactsPage from "./pages/ArtifactsPage";
import RepoArtifactsPage from "./pages/RepoArtifactsPage";
import RepoEventsPage from "./pages/RepoEventsPage";
import RepoLogsPage from "./pages/RepoLogsPage";
import IssuesPage from "./pages/IssuesPage";
import PullRequestsPage from "./pages/PullRequestsPage";
import ReleasesPage from "./pages/ReleasesPage";
import SettingsPage from "./pages/SettingsPage";

export default function AppRoutes() {
  return (
    <Routes>
      <Route path="/" element={<DashboardPage />} />
      <Route path="/analytics/:repoId" element={<AnalyticsPage />} />
      <Route path="/repos" element={<Navigate to="/" replace />} />
      <Route path="/repos/connect" element={<RepoConnectPage />} />
      <Route path="/repos/:repoId/settings" element={<RepoSettingsPage />} />
      <Route path="/repos/:repoId/workflows" element={<WorkflowListPage />} />
      <Route path="/repos/:repoId/workflows/:workflowId" element={<WorkflowEditorPage />} />
      <Route path="/repos/:repoId/runs" element={<RunListPage />} />
      <Route path="/runs/:runId" element={<RunDetailPage />} />
      <Route path="/runs/:runId/artifacts" element={<ArtifactsPage />} />
      <Route path="/repos/:repoId/logs" element={<RepoLogsPage />} />
      <Route path="/repos/:repoId/artifacts" element={<RepoArtifactsPage />} />
      <Route path="/repos/:repoId/events" element={<RepoEventsPage />} />
      <Route path="/repos/:repoId/issues" element={<IssuesPage />} />
      <Route path="/repos/:repoId/pulls" element={<PullRequestsPage />} />
      <Route path="/repos/:repoId/releases" element={<ReleasesPage />} />
      <Route path="/settings" element={<SettingsPage />} />
      <Route path="*" element={<Navigate to="/" replace />} />
    </Routes>
  );
}
