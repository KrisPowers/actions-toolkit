-- github_status reporting failures (start_check/report_pending/report_success/report_failure/
-- complete_check) were only ever tracing::warn!'d, invisible anywhere in the product itself.
-- Persisting the last such error onto the run lets the run detail page surface it directly
-- instead of requiring access to the server's own console output.

ALTER TABLE workflow_runs ADD COLUMN github_report_error TEXT;
