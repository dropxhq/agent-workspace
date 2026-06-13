use std::path::PathBuf;

use serde::Deserialize;

pub const DEFAULT_METADATA_SUFFIX: &str = ".meta.yaml";
pub const DEFAULT_MYSQL_PORT: u16 = 3306;

fn default_metadata_suffix() -> String {
    DEFAULT_METADATA_SUFFIX.to_string()
}

fn default_mysql_port() -> u16 {
    DEFAULT_MYSQL_PORT
}

fn default_type_file() -> String {
    "file".to_string()
}

fn default_type_mysql() -> String {
    "mysql".to_string()
}

#[derive(Debug, Deserialize)]
pub struct RawFileBackend {
    #[serde(default = "default_type_file")]
    pub r#type: String,
    pub workspace_dir: PathBuf,
    #[serde(default = "default_metadata_suffix")]
    pub metadata_suffix: String,
}

#[derive(Debug, Deserialize)]
pub struct RawMysqlBackend {
    #[serde(default = "default_type_mysql")]
    pub r#type: String,
    pub host: String,
    #[serde(default = "default_mysql_port")]
    pub port: u16,
    pub user: String,
    pub password: String,
    pub database: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum RawBackend {
    Wrapped { backend: RawBackendInner },
    File(RawFileBackend),
    Mysql(RawMysqlBackend),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum RawBackendInner {
    File(RawFileBackend),
    Mysql(RawMysqlBackend),
}
