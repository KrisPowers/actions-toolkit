use sqlx::SqlitePool;

use crate::models::GithubInstallation;

pub async fn list(pool: &SqlitePool) -> sqlx::Result<Vec<GithubInstallation>> {
    sqlx::query_as::<_, GithubInstallation>("SELECT * FROM github_installations ORDER BY account_login").fetch_all(pool).await
}
