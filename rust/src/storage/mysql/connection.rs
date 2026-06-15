use std::future::Future;

use chrono::{DateTime, FixedOffset, NaiveDateTime, Utc};
use sqlx::mysql::{MySqlConnectOptions, MySqlPoolOptions};
use sqlx::MySqlPool;
use tokio::runtime::{Builder, Runtime};

use crate::error::{WsError, WsResult};

pub(super) const MYSQL_METADATA_SUFFIX: &str = ".meta.yaml";

pub(super) fn map_db_err(e: sqlx::Error) -> WsError {
    if let sqlx::Error::Database(db_err) = &e {
        if matches!(
            db_err.code().map(|c| c.to_string()).as_deref(),
            Some("1205") | Some("1213")
        ) {
            return WsError::LockConflict(db_err.message().to_string());
        }
    }
    let msg = e.to_string();
    if msg.contains("Lock wait timeout") {
        return WsError::LockConflict(msg);
    }
    WsError::Other(msg)
}

fn quote_mysql_identifier(name: &str) -> WsResult<String> {
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(WsError::Other(format!(
            "invalid mysql identifier: '{name}', only [A-Za-z0-9_] is allowed"
        )));
    }
    Ok(format!("`{name}`"))
}

pub(super) fn naive_utc_to_fixed(value: NaiveDateTime) -> DateTime<FixedOffset> {
    DateTime::<Utc>::from_naive_utc_and_offset(value, Utc).fixed_offset()
}

pub struct MySqlBackend {
    pub(super) pool: MySqlPool,
    runtime: Option<Runtime>,
}

impl MySqlBackend {
    pub fn ensure_schema(
        host: &str,
        port: u16,
        user: &str,
        password: &str,
        database: &str,
    ) -> WsResult<()> {
        let runtime = Runtime::new()
            .map_err(|e| WsError::Other(format!("failed to create tokio runtime: {e}")))?;

        let base_options = MySqlConnectOptions::new()
            .host(host)
            .port(port)
            .username(user)
            .password(password);
        let admin_pool = MySqlPoolOptions::new()
            .max_connections(2)
            .connect_lazy_with(base_options);

        runtime.block_on(async {
            sqlx::query("SELECT 1")
                .execute(&admin_pool)
                .await
                .map_err(map_db_err)?;
            Self::ensure_schema_with_pool(&admin_pool, database).await
        })
    }

    pub fn connect(
        host: &str,
        port: u16,
        user: &str,
        password: &str,
        database: &str,
    ) -> WsResult<Self> {
        let runtime = Runtime::new()
            .map_err(|e| WsError::Other(format!("failed to create tokio runtime: {e}")))?;

        let base_options = MySqlConnectOptions::new()
            .host(host)
            .port(port)
            .username(user)
            .password(password);

        let admin_pool = MySqlPoolOptions::new()
            .max_connections(2)
            .connect_lazy_with(base_options.clone());

        let mut backend = Self {
            pool: admin_pool.clone(),
            runtime: Some(runtime),
        };

        backend.block_on(async {
            sqlx::query("SELECT 1")
                .execute(&admin_pool)
                .await
                .map_err(map_db_err)?;
            Self::ensure_schema_with_pool(&admin_pool, database).await?;
            Ok(())
        })?;

        let db_pool = MySqlPoolOptions::new()
            .max_connections(5)
            .connect_lazy_with(base_options.database(database));
        backend.block_on(async {
            sqlx::query("SELECT 1")
                .execute(&db_pool)
                .await
                .map_err(map_db_err)?;
            Ok(())
        })?;
        backend.pool = db_pool;

        Ok(backend)
    }

    pub(super) fn block_on<F, T>(&self, future: F) -> WsResult<T>
    where
        F: Future<Output = WsResult<T>>,
    {
        if let Some(runtime) = &self.runtime {
            return runtime.block_on(future);
        }

        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| WsError::Other(format!("failed to build runtime: {e}")))?;
        runtime.block_on(future)
    }

    async fn ensure_schema_with_pool(pool: &MySqlPool, database: &str) -> WsResult<()> {
        let database_ident = quote_mysql_identifier(database)?;
        sqlx::query(&format!("CREATE DATABASE IF NOT EXISTS {database_ident}"))
            .execute(pool)
            .await
            .map_err(map_db_err)?;

        sqlx::query(&format!(
            "CREATE TABLE IF NOT EXISTS {database_ident}.workspace_files (
                relative_path VARCHAR(1024) NOT NULL PRIMARY KEY,
                content LONGTEXT NOT NULL,
                created_by VARCHAR(255) NOT NULL DEFAULT '',
                description TEXT NOT NULL,
                created_at DATETIME(6) NOT NULL,
                updated_at DATETIME(6) NOT NULL,
                size_bytes BIGINT UNSIGNED NOT NULL,
                sha256 CHAR(64) NULL
            ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci"
        ))
        .execute(pool)
        .await
        .map_err(map_db_err)?;

        Ok(())
    }
}
