pub mod metadata_name;
pub mod normalize;
pub mod resolve;
pub mod scope_prefix;

pub use metadata_name::{data_path_from_metadata, is_metadata_path, metadata_path_for};
pub use normalize::normalize_workspace_relative;
pub use resolve::{
    parse_ws_path, parse_ws_path_for_write, parse_ws_path_for_write_in, parse_ws_path_in,
    resolve_relative, resolve_relative_in, validate_within_workspace, ResolvedPath,
};
pub use scope_prefix::{list_scope_prefix, path_matches_scope};
