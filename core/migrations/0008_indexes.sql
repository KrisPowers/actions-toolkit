CREATE INDEX idx_workflow_runs_workflow ON workflow_runs(workflow_id, created_at DESC);
CREATE INDEX idx_workflow_runs_repo ON workflow_runs(repo_id, created_at DESC);
CREATE INDEX idx_job_runs_run ON job_runs(workflow_run_id);
CREATE INDEX idx_step_runs_job ON step_runs(job_run_id);
CREATE INDEX idx_artifacts_run ON artifacts(workflow_run_id);
CREATE INDEX idx_webhook_events_repo ON webhook_events(repo_id, received_at DESC);
