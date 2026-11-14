//! Small cross-cutting helpers shared by multiple modules (DRY). Each concern lives in its own
//! module; this file re-exports them so every caller keeps the one `util::` path.

mod encoding;
mod executables;
mod files;
mod ipc;
mod json;
mod paths;
mod process;
mod project_encoding;
#[cfg(windows)]
mod registry;
#[cfg(test)]
mod tests;
mod time;

pub(crate) use self::encoding::{lowercase_hex, percent_encode};
pub(crate) use self::executables::{find_in, is_on_path, resolve, search_directories};
pub(crate) use self::files::{newest_in_tail, write_if_stale};
pub(crate) use self::ipc::{
    owner_access, OwnerAccess, MAXIMUM_PATH_BYTES, MAXIMUM_SESSION_ID_BYTES,
};
pub(crate) use self::json::json_string_field;
pub(crate) use self::json::{read_json, read_jsonc};
pub(crate) use self::paths::{comparable_path, same_path};
pub(crate) use self::paths::{home_directory, is_noise_directory, pade_data_directory};
pub(crate) use self::process::{capture_within, COMMAND_SHELL, COMMAND_SHELL_RUN_FLAG};
pub(crate) use self::process::{
    command, exits_successfully_within, shell_command, spawn_detached, succeeds_within,
};
pub(crate) use self::project_encoding::encode_project;
#[cfg(windows)]
pub(crate) use self::registry::{registry_query, registry_string};
pub(crate) use self::time::{
    days_since_epoch, milliseconds_since_unix_epoch, now_milliseconds, since_unix_epoch,
};
