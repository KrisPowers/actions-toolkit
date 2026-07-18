use sqlx::SqlitePool;

use crate::db::models::{now_iso, GithubToken};

pub async fn get(pool: &SqlitePool) -> sqlx::Result<Option<GithubToken>> {
    sqlx::query_as::<_, GithubToken>("SELECT * FROM github_token WHERE id = 1")
        .fetch_optional(pool)
        .await
}

pub async fn upsert(
    pool: &SqlitePool,
    token_encrypted: &[u8],
    token_nonce: &[u8],
    github_login: &str,
    scopes: &str,
) -> sqlx::Result<GithubToken> {
    let now = now_iso();
    sqlx::query(
        "INSERT INTO github_token (id, token_encrypted, token_nonce, github_login, scopes, created_at, updated_at) \
         VALUES (1, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(id) DO UPDATE SET \
            token_encrypted = excluded.token_encrypted, \
            token_nonce = excluded.token_nonce, \
            github_login = excluded.github_login, \
            scopes = excluded.scopes, \
            updated_at = excluded.updated_at",
    )
    .bind(token_encrypted)
    .bind(token_nonce)
    .bind(github_login)
    .bind(scopes)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    get(pool).await?.ok_or(sqlx::Error::RowNotFound)
}

pub async fn delete(pool: &SqlitePool) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM github_token WHERE id = 1").execute(pool).await?;
    Ok(())
}

/// Upsert a GitHub App user-to-server token (`token_type = 'github_app'`). Replaces whatever was
/// in the singleton row before, including a legacy PAT: the old token is gone the moment this
/// call succeeds, there's nothing left to separately delete.
#[allow(clippy::too_many_arguments)]
pub async fn upsert_app_token(
    pool: &SqlitePool,
    token_encrypted: &[u8],
    token_nonce: &[u8],
    refresh_token_encrypted: &[u8],
    refresh_token_nonce: &[u8],
    expires_at: &str,
    installation_id: Option<i64>,
    github_login: &str,
) -> sqlx::Result<GithubToken> {
    let now = now_iso();
    sqlx::query(
        "INSERT INTO github_token \
            (id, token_encrypted, token_nonce, github_login, scopes, created_at, updated_at, \
             token_type, refresh_token_encrypted, refresh_token_nonce, expires_at, \
             installation_id, needs_reconnect) \
         VALUES (1, ?, ?, ?, '', ?, ?, 'github_app', ?, ?, ?, ?, 0) \
         ON CONFLICT(id) DO UPDATE SET \
            token_encrypted = excluded.token_encrypted, \
            token_nonce = excluded.token_nonce, \
            github_login = excluded.github_login, \
            scopes = excluded.scopes, \
            updated_at = excluded.updated_at, \
            token_type = excluded.token_type, \
            refresh_token_encrypted = excluded.refresh_token_encrypted, \
            refresh_token_nonce = excluded.refresh_token_nonce, \
            expires_at = excluded.expires_at, \
            installation_id = excluded.installation_id, \
            needs_reconnect = 0",
    )
    .bind(token_encrypted)
    .bind(token_nonce)
    .bind(github_login)
    .bind(&now)
    .bind(&now)
    .bind(refresh_token_encrypted)
    .bind(refresh_token_nonce)
    .bind(expires_at)
    .bind(installation_id)
    .execute(pool)
    .await?;

    get(pool).await?.ok_or(sqlx::Error::RowNotFound)
}

/// Persist a refreshed access token (and, if GitHub rotated it, refresh token) plus new expiry
/// for the existing `github_app` row, clearing any prior `needs_reconnect` flag.
pub async fn update_after_refresh(
    pool: &SqlitePool,
    token_encrypted: &[u8],
    token_nonce: &[u8],
    refresh_token_encrypted: &[u8],
    refresh_token_nonce: &[u8],
    expires_at: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE github_token SET \
            token_encrypted = ?, token_nonce = ?, \
            refresh_token_encrypted = ?, refresh_token_nonce = ?, \
            expires_at = ?, needs_reconnect = 0, updated_at = ? \
         WHERE id = 1",
    )
    .bind(token_encrypted)
    .bind(token_nonce)
    .bind(refresh_token_encrypted)
    .bind(refresh_token_nonce)
    .bind(expires_at)
    .bind(now_iso())
    .execute(pool)
    .await?;
    Ok(())
}

/// Mark the connection as needing reconnect (a refresh attempt failed: revoked/expired refresh
/// token, or the row is a legacy `pat` that was never eligible for refresh in the first place).
pub async fn mark_needs_reconnect(pool: &SqlitePool) -> sqlx::Result<()> {
    sqlx::query("UPDATE github_token SET needs_reconnect = 1, updated_at = ? WHERE id = 1")
        .bind(now_iso())
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::EncryptionKey;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    /// Rule-proving test: an app token round-trips through `EncryptionKey` only. The raw sqlite
    /// blob columns must never equal the plaintext token or refresh token, i.e. nothing bypassed
    /// encryption on the way in.
    #[tokio::test]
    async fn app_token_is_unreadable_in_the_database_without_the_encryption_key() {
        let pool = test_pool().await;
        let enc = EncryptionKey::load_or_generate(None, &std::env::temp_dir().join("atk-github-token-test")).unwrap();

        let plaintext_access = "ghu_real_access_token_value";
        let plaintext_refresh = "ghr_real_refresh_token_value";
        let (access_ct, access_nonce) = enc.encrypt_str(plaintext_access).unwrap();
        let (refresh_ct, refresh_nonce) = enc.encrypt_str(plaintext_refresh).unwrap();

        upsert_app_token(&pool, &access_ct, &access_nonce, &refresh_ct, &refresh_nonce, "2099-01-01T00:00:00Z", Some(42), "octocat")
            .await
            .unwrap();

        let raw: (Vec<u8>, Vec<u8>) =
            sqlx::query_as("SELECT token_encrypted, refresh_token_encrypted FROM github_token WHERE id = 1")
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_ne!(raw.0, plaintext_access.as_bytes());
        assert_ne!(raw.1, plaintext_refresh.as_bytes());
        assert_eq!(enc.decrypt_str(&raw.0, &access_nonce).unwrap(), plaintext_access);
        assert_eq!(enc.decrypt_str(&raw.1, &refresh_nonce).unwrap(), plaintext_refresh);
    }

    #[tokio::test]
    async fn upsert_app_token_replaces_a_legacy_pat_row_outright() {
        let pool = test_pool().await;
        upsert(&pool, b"pat-ciphertext", b"pat-nonce", "octocat", "repo").await.unwrap();

        upsert_app_token(&pool, b"app-ciphertext", b"app-nonce", b"refresh-ciphertext", b"refresh-nonce", "2099-01-01T00:00:00Z", Some(7), "octocat")
            .await
            .unwrap();

        let row = get(&pool).await.unwrap().unwrap();
        assert_eq!(row.token_type, "github_app");
        assert_eq!(row.token_encrypted, b"app-ciphertext");
        assert_eq!(row.needs_reconnect, 0);
        assert_eq!(row.installation_id, Some(7));
    }
}
