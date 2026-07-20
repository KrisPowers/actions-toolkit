import { lazy, Suspense } from "react";
import { Navigate, Route, Routes } from "react-router-dom";
import { Loader2 } from "lucide-react";

// Lazy-loaded per route so each page (and whatever it pulls in, e.g. Monaco + React Flow for
// the workflow editor, Recharts for analytics) becomes its own chunk instead of all landing in
// the single chunk loaded on first paint.
const DashboardPage = lazy(() => import("./pages/DashboardPage"));
const AnalyticsPage = lazy(() => import("./pages/AnalyticsPage"));
const RepoConnectPage = lazy(() => import("./pages/RepoConnectPage"));
const RepoSettingsLayout = lazy(() => import("./pages/RepoSettingsLayout"));
const RepoWebhooksPage = lazy(() => import("./pages/settings/RepoWebhooksPage"));
const RepoSecretsSettingsPage = lazy(() => import("./pages/settings/RepoSecretsSettingsPage"));
const RepoAccessSettingsPage = lazy(() => import("./pages/settings/RepoAccessSettingsPage"));
const RepoDataSettingsPage = lazy(() => import("./pages/settings/RepoDataSettingsPage"));
const RepoDangerSettingsPage = lazy(() => import("./pages/settings/RepoDangerSettingsPage"));
const WorkflowListPage = lazy(() => import("./pages/WorkflowListPage"));
const WorkflowEditorPage = lazy(() => import("./pages/WorkflowEditorPage"));
const RunListPage = lazy(() => import("./pages/RunListPage"));
const RunDetailPage = lazy(() => import("./pages/RunDetailPage"));
const ArtifactsPage = lazy(() => import("./pages/ArtifactsPage"));
const RepoArtifactsPage = lazy(() => import("./pages/RepoArtifactsPage"));
const RepoEventsPage = lazy(() => import("./pages/RepoEventsPage"));
const RepoLogsPage = lazy(() => import("./pages/RepoLogsPage"));
const SettingsPage = lazy(() => import("./pages/SettingsPage"));

function RouteFallback() {
  return (
    <div className="flex h-full w-full items-center justify-center py-24">
      <Loader2 className="h-5 w-5 animate-spin text-neutral-500" strokeWidth={2} />
    </div>
  );
}

export default function AppRoutes() {
  return (
    <Suspense fallback={<RouteFallback />}>
      <Routes>
        <Route path="/" element={<DashboardPage />} />
        <Route path="/analytics/:repoId" element={<AnalyticsPage />} />
        <Route path="/repos" element={<Navigate to="/" replace />} />
        <Route path="/repos/connect" element={<RepoConnectPage />} />
        <Route path="/repos/:repoId/settings" element={<RepoSettingsLayout />}>
          <Route index element={<Navigate to="webhooks" replace />} />
          <Route path="webhooks" element={<RepoWebhooksPage />} />
          <Route path="secrets" element={<RepoSecretsSettingsPage />} />
          <Route path="access" element={<RepoAccessSettingsPage />} />
          <Route path="data" element={<RepoDataSettingsPage />} />
          <Route path="danger" element={<RepoDangerSettingsPage />} />
        </Route>
        <Route path="/repos/:repoId/workflows" element={<WorkflowListPage />} />
        <Route path="/repos/:repoId/workflows/:workflowId" element={<WorkflowEditorPage />} />
        <Route path="/repos/:repoId/runs" element={<RunListPage />} />
        <Route path="/runs/:runId" element={<RunDetailPage />} />
        <Route path="/runs/:runId/artifacts" element={<ArtifactsPage />} />
        <Route path="/repos/:repoId/logs" element={<RepoLogsPage />} />
        <Route path="/repos/:repoId/artifacts" element={<RepoArtifactsPage />} />
        <Route path="/repos/:repoId/events" element={<RepoEventsPage />} />
        <Route path="/settings" element={<SettingsPage />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </Suspense>
  );
}
