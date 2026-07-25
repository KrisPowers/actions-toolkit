use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::{now_iso, DashboardTunnelRequest};

pub struct NewRequest {
    pub user_id: Option<String>,
    pub ip_address: Option<String>,
    pub method: String,
    pub path: String,
    pub status_code: i64,
    pub rate_limited: bool,
}

/// Inserts a batch inside a single transaction, called periodically by the dashboard tunnel's
/// buffered flush (see `tunnel::dashboard_manager`) rather than once per request, so a request
/// flood can't turn into a write storm on top of the traffic itself.
pub async fn record_batch(pool: &SqlitePool, entries: &[NewRequest]) -> sqlx::Result<()> {
    if entries.is_empty() {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    let now = now_iso();
    for entry in entries {
        sqlx::query(
            "INSERT INTO dashboard_tunnel_requests (id, user_id, ip_address, method, path, status_code, rate_limited, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&entry.user_id)
        .bind(&entry.ip_address)
        .bind(&entry.method)
        .bind(&entry.path)
        .bind(entry.status_code)
        .bind(entry.rate_limited as i64)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn list_recent(pool: &SqlitePool, limit: i64, offset: i64) -> sqlx::Result<Vec<DashboardTunnelRequest>> {
    sqlx::query_as::<_, DashboardTunnelRequest>("SELECT * FROM dashboard_tunnel_requests ORDER BY created_at DESC LIMIT ? OFFSET ?")
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
}
