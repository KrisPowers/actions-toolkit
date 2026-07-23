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
const RepoWebhooksPage = lazy(() => import("./pages/RepoWebhooksPage"));
const RepoSecretsSettingsPage = lazy(() => import("./pages/settings/RepoSecretsSettingsPage"));
const RepoAccessSettingsPage = lazy(() => import("./pages/settings/RepoAccessSettingsPage"));
const RepoDataSettingsPage = lazy(() => import("./pages/settings/RepoDataSettingsPage"));
const RepoDangerSettingsPage = lazy(() => import("./pages/settings/RepoDangerSettingsPage"));
const OverviewPage = lazy(() => import("./pages/OverviewPage"));
const WorkflowEditorPage = lazy(() => import("./pages/WorkflowEditorPage"));
const RunDetailLayout = lazy(() => import("./pages/RunDetailLayout"));
const RunLogsPanel = lazy(() => import("./pages/runs/RunLogsPanel"));
const RunArtifactsPanel = lazy(() => import("./pages/runs/RunArtifactsPanel"));
const RunInsightsPanel = lazy(() => import("./pages/runs/RunInsightsPanel"));
const RunBackendPanel = lazy(() => import("./pages/runs/RunBackendPanel"));
const BucketDetailPage = lazy(() => import("./pages/BucketDetailPage"));
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
          <Route index element={<Navigate to="secrets" replace />} />
          <Route path="secrets" element={<RepoSecretsSettingsPage />} />
          <Route path="access" element={<RepoAccessSettingsPage />} />
          <Route path="data" element={<RepoDataSettingsPage />} />
          <Route path="danger" element={<RepoDangerSettingsPage />} />
        </Route>
        <Route path="/repos/:repoId/overview" element={<OverviewPage />} />
        <Route path="/repos/:repoId/workflows" element={<Navigate to="../overview" replace />} />
        <Route path="/repos/:repoId/workflows/:workflowId" element={<WorkflowEditorPage />} />
        <Route path="/repos/:repoId/runs" element={<Navigate to="../overview" replace />} />
        <Route path="/runs/:runId" element={<RunDetailLayout />}>
          <Route index element={<Navigate to="logs" replace />} />
          <Route path="logs" element={<RunLogsPanel />} />
          <Route path="artifacts" element={<RunArtifactsPanel />} />
          <Route path="insights" element={<RunInsightsPanel />} />
          <Route path="backend" element={<RunBackendPanel />} />
        </Route>
        <Route path="/buckets/:bucketId" element={<BucketDetailPage />} />
        <Route path="/repos/:repoId/webhooks" element={<RepoWebhooksPage />} />
        <Route path="/settings" element={<SettingsPage />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </Suspense>
  );
}
