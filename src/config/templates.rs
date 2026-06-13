pub const DEFAULT_FILE_CONFIG: &str = r#"backend:
  type: file
  workspace_dir: ./data
  metadata_suffix: ".meta.yaml"
"#;

pub const DEFAULT_MYSQL_CONFIG: &str = r#"backend:
  type: mysql
  host: localhost
  port: 3306
  user: ws_user
  password: change_me
  database: agent_workspace
"#;
