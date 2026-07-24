use sqlx::SqlitePool;

use crate::models::{now_iso, GithubInstallation};

pub async fn list(pool: &SqlitePool) -> sqlx::Result<Vec<GithubInstallation>> {
    sqlx::query_as::<_, GithubInstallation>("SELECT * FROM github_installations ORDER BY account_login").fetch_all(pool).await
}

/// Replaces the whole set wholesale (delete + reinsert) rather than diffing: this is a discovery
/// cache of what GitHub currently reports, not a credential, so there's no history worth
/// preserving across a reconnect or an explicit refresh.
pub async fn upsert_all(pool: &SqlitePool, installations: &[(i64, String, String, Option<String>)]) -> sqlx::Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM github_installations").execute(&mut *tx).await?;
    let now = now_iso();
    for (id, account_login, account_type, app_slug) in installations {
        sqlx::query("INSERT INTO github_installations (id, account_login, account_type, app_slug, connected_at) VALUES (?, ?, ?, ?, ?)")
            .bind(id)
            .bind(account_login)
            .bind(account_type)
            .bind(app_slug)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}
