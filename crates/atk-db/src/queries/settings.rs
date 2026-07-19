use sqlx::SqlitePool;

use crate::db::models::{now_iso, Settings};

pub async fn get(pool: &SqlitePool) -> sqlx::Result<Settings> {
    sqlx::query_as::<_, Settings>("SELECT * FROM settings WHERE id = 1").fetch_one(pool).await
}

#[derive(Debug, Default, Clone)]
pub struct SettingsPatch {
    pub port: Option<u16>,
    pub bind_addr: Option<String>,
    /// `Some(None)` clears the override back to auto-detect; `Some(Some(host))` sets it;
    /// `None` leaves the current value untouched.
    pub docker_host: Option<Option<String>>,
    pub max_concurrent_jobs: Option<usize>,
}

impl SettingsPatch {
    pub fn is_empty(&self) -> bool {
        self.port.is_none() && self.bind_addr.is_none() && self.docker_host.is_none() && self.max_concurrent_jobs.is_none()
    }
}

pub async fn update(pool: &SqlitePool, patch: SettingsPatch) -> sqlx::Result<Settings> {
    let current = get(pool).await?;

    let port = patch.port.map(i64::from).unwrap_or(current.port);
    let bind_addr = patch.bind_addr.unwrap_or(current.bind_addr);
    let docker_host = patch.docker_host.unwrap_or(current.docker_host);
    let max_concurrent_jobs = patch.max_concurrent_jobs.map(|v| v as i64).unwrap_or(current.max_concurrent_jobs);
    let now = now_iso();

    sqlx::query(
        "UPDATE settings SET port = ?, bind_addr = ?, docker_host = ?, max_concurrent_jobs = ?, updated_at = ? \
         WHERE id = 1",
    )
    .bind(port)
    .bind(&bind_addr)
    .bind(&docker_host)
    .bind(max_concurrent_jobs)
    .bind(&now)
    .execute(pool)
    .await?;

    get(pool).await
}
