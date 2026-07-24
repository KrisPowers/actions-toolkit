use sqlx::SqlitePool;

use crate::models::DashboardTunnel;

/// The singleton row, seeded by migration `0034`, so it always exists once the database has
/// been created (same pattern as `settings::get`).
pub async fn get(pool: &SqlitePool) -> sqlx::Result<DashboardTunnel> {
    sqlx::query_as::<_, DashboardTunnel>("SELECT * FROM dashboard_tunnel WHERE id = 1").fetch_one(pool).await
}
