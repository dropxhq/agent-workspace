use chrono::{NaiveDateTime, Utc};
use sqlx::Row;

use crate::error::{WsError, WsResult};
use crate::metadata::{compute_sha256, FileMetadata};
use crate::paths::{is_metadata_path, list_scope_prefix, normalize_workspace_relative};
use crate::ranges::{apply_write_ranges, filter_lines, LineRange};
use crate::storage::{ListReport, WorkspaceBackend};

mod connection;

pub use connection::MySqlBackend;

use connection::{map_db_err, naive_utc_to_fixed, MYSQL_METADATA_SUFFIX};

impl WorkspaceBackend for MySqlBackend {
    fn read(&self, path: &str, ranges: Option<&[crate::ranges::LineRange]>) -> WsResult<String> {
        let relative = normalize_workspace_relative(path);
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
        let relative = normalize_workspace_relative(path);
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
                .map(normalize_workspace_relative)
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
        let relative = normalize_workspace_relative(path);
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
