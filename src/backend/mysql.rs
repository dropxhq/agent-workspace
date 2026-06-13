use std::future::Future;

use chrono::{DateTime, FixedOffset, NaiveDateTime, Utc};
use sqlx::mysql::{MySqlConnectOptions, MySqlPoolOptions};
use sqlx::{MySqlPool, Row};
use tokio::runtime::{Builder, Runtime};

use crate::backend::content::filter_lines;
use crate::backend::path::{is_metadata_path, list_scope_prefix, normalize_input_path};
use crate::backend::{ListReport, WorkspaceBackend};
use crate::commands::ranges::{apply_write_ranges, LineRange};
use crate::error::{WsError, WsResult};
use crate::meta::{compute_sha256, FileMetadata};

const MYSQL_METADATA_SUFFIX: &str = ".meta.yaml";

fn map_db_err(e: sqlx::Error) -> WsError {
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

fn naive_utc_to_fixed(value: NaiveDateTime) -> DateTime<FixedOffset> {
    DateTime::<Utc>::from_naive_utc_and_offset(value, Utc).fixed_offset()
}

pub struct MySqlBackend {
    pool: MySqlPool,
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

    fn block_on<F, T>(&self, future: F) -> WsResult<T>
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

impl WorkspaceBackend for MySqlBackend {
    fn read(
        &self,
        path: &str,
        ranges: Option<&[crate::commands::ranges::LineRange]>,
    ) -> WsResult<String> {
        let relative = normalize_input_path(path);
        if is_metadata_path(&relative, MYSQL_METADATA_SUFFIX) {
            return Err(WsError::NotFound(relative));
        }

        let content = self.block_on(async {
            sqlx::query_scalar::<_, String>(
                "SELECT content FROM workspace_files WHERE relative_path = ?",
            )
            .bind(&relative)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_db_err)
        })?;

        let content = content.ok_or_else(|| WsError::NotFound(relative.clone()))?;
        if let Some(ranges) = ranges {
            Ok(filter_lines(&content, ranges))
        } else {
            Ok(content)
        }
    }

    fn write(
        &self,
        path: &str,
        ranges: Option<&LineRange>,
        content: &str,
        created_by: &str,
        desc: &str,
    ) -> WsResult<()> {
        let relative = normalize_input_path(path);
        if is_metadata_path(&relative, MYSQL_METADATA_SUFFIX) {
            return Err(WsError::NotFound(relative));
        }

        self.block_on(async {
            let mut tx = self.pool.begin().await.map_err(map_db_err)?;

            let existing = sqlx::query(
                "SELECT content, created_by, created_at FROM workspace_files WHERE relative_path = ? FOR UPDATE",
            )
            .bind(&relative)
            .fetch_optional(&mut *tx)
            .await
            .map_err(map_db_err)?;
            let is_update = existing.is_some();

            let (final_content, insert_created_by, insert_created_at) = if let Some(row) = existing {
                let existing_content: String = row.try_get("content").map_err(map_db_err)?;
                let original_created_by: String = row.try_get("created_by").map_err(map_db_err)?;
                let original_created_at: NaiveDateTime =
                    row.try_get("created_at").map_err(map_db_err)?;

                let merged = if let Some(range) = ranges {
                    apply_write_ranges(&existing_content, range, content)
                } else {
                    content.to_string()
                };
                (merged, original_created_by, original_created_at)
            } else {
                let merged = if let Some(range) = ranges {
                    apply_write_ranges("", range, content)
                } else {
                    content.to_string()
                };
                (merged, created_by.to_string(), Utc::now().naive_utc())
            };

            let now = Utc::now().naive_utc();
            let size_bytes = final_content.len() as u64;
            let sha256 = compute_sha256(final_content.as_bytes());

            if is_update {
                sqlx::query(
                    "UPDATE workspace_files
                     SET content = ?, description = ?, updated_at = ?, size_bytes = ?, sha256 = ?
                     WHERE relative_path = ?",
                )
                .bind(&final_content)
                .bind(desc)
                .bind(now)
                .bind(size_bytes)
                .bind(sha256)
                .bind(&relative)
                .execute(&mut *tx)
                .await
                .map_err(map_db_err)?;
            } else {
                sqlx::query(
                    "INSERT INTO workspace_files
                     (relative_path, content, created_by, description, created_at, updated_at, size_bytes, sha256)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&relative)
                .bind(&final_content)
                .bind(insert_created_by)
                .bind(desc)
                .bind(insert_created_at)
                .bind(now)
                .bind(size_bytes)
                .bind(sha256)
                .execute(&mut *tx)
                .await
                .map_err(map_db_err)?;
            }

            tx.commit().await.map_err(map_db_err)?;
            Ok(())
        })
    }

    fn list(&self, scope: Option<&str>) -> WsResult<ListReport> {
        let report_scope =
            scope
                .map(normalize_input_path)
                .and_then(|s| if s.is_empty() { None } else { Some(s) });
        let scope_prefix = list_scope_prefix(report_scope.as_deref());

        let files: Vec<FileMetadata> = self.block_on(async {
            let rows = if let Some(prefix) = &scope_prefix {
                let exact = prefix.trim_end_matches('/').to_string();
                let like = format!("{prefix}%");
                sqlx::query(
                    "SELECT relative_path, created_by, description, created_at, updated_at, size_bytes, sha256
                     FROM workspace_files
                     WHERE relative_path = ? OR relative_path LIKE ?
                     ORDER BY relative_path",
                )
                .bind(exact)
                .bind(like)
                .fetch_all(&self.pool)
                .await
                .map_err(map_db_err)?
            } else {
                sqlx::query(
                    "SELECT relative_path, created_by, description, created_at, updated_at, size_bytes, sha256
                     FROM workspace_files
                     ORDER BY relative_path",
                )
                .fetch_all(&self.pool)
                .await
                .map_err(map_db_err)?
            };

            rows.into_iter()
                .map(|row| {
                    let created_at: NaiveDateTime = row.try_get("created_at").map_err(map_db_err)?;
                    let updated_at: NaiveDateTime = row.try_get("updated_at").map_err(map_db_err)?;
                    Ok(FileMetadata {
                        relative_path: row.try_get("relative_path").map_err(map_db_err)?,
                        created_by: row.try_get("created_by").map_err(map_db_err)?,
                        desc: row.try_get("description").map_err(map_db_err)?,
                        created_at: naive_utc_to_fixed(created_at),
                        updated_at: naive_utc_to_fixed(updated_at),
                        size_bytes: row.try_get("size_bytes").map_err(map_db_err)?,
                        sha256: row.try_get("sha256").map_err(map_db_err)?,
                    })
                })
                .collect::<WsResult<Vec<_>>>()
        })?;

        let total_size_bytes = files.iter().map(|f| f.size_bytes).sum();
        Ok(ListReport {
            scope: report_scope,
            file_count: files.len(),
            total_size_bytes,
            files,
        })
    }

    fn remove(&self, path: &str) -> WsResult<()> {
        let relative = normalize_input_path(path);
        if is_metadata_path(&relative, MYSQL_METADATA_SUFFIX) {
            return Err(WsError::NotFound(relative));
        }

        self.block_on(async {
            let mut tx = self.pool.begin().await.map_err(map_db_err)?;
            let result = sqlx::query("DELETE FROM workspace_files WHERE relative_path = ?")
                .bind(&relative)
                .execute(&mut *tx)
                .await
                .map_err(map_db_err)?;

            if result.rows_affected() == 0 {
                tx.rollback().await.map_err(map_db_err)?;
                return Err(WsError::NotFound(relative.clone()));
            }

            tx.commit().await.map_err(map_db_err)?;
            Ok(())
        })
    }
}
