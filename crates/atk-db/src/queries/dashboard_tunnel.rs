use sqlx::SqlitePool;

use crate::models::{now_iso, DashboardTunnel};

/// The singleton row, seeded by migration `0034`, so it always exists once the database has
/// been created (same pattern as `settings::get`).
pub async fn get(pool: &SqlitePool) -> sqlx::Result<DashboardTunnel> {
    sqlx::query_as::<_, DashboardTunnel>("SELECT * FROM dashboard_tunnel WHERE id = 1").fetch_one(pool).await
}

pub async fn update(pool: &SqlitePool, provider: &str, enabled: bool, local_port: Option<i64>, last_url: Option<&str>) -> sqlx::Result<DashboardTunnel> {
    sqlx::query("UPDATE dashboard_tunnel SET provider = ?, enabled = ?, local_port = ?, last_url = ?, updated_at = ? WHERE id = 1")
        .bind(provider)
        .bind(enabled as i64)
        .bind(local_port)
        .bind(last_url)
        .bind(now_iso())
        .execute(pool)
        .await?;
    get(pool).await
}
