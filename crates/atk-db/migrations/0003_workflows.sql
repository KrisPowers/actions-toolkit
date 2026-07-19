CREATE TABLE workflows (
    id              TEXT PRIMARY KEY,
    repo_id         TEXT NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    file_path       TEXT NOT NULL,
    yaml_source     TEXT NOT NULL,
    parsed_json     TEXT NOT NULL,
    enabled         INTEGER NOT NULL DEFAULT 1,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    UNIQUE(repo_id, name)
);
