pub mod commands;
pub mod error;
pub mod interactive;
pub mod language;
pub mod platform;
pub mod presentation;
pub mod runtime;

/// New names take precedence, including an explicitly empty value. Preserve
/// existing installations' startup settings while the product is renamed.
pub(crate) fn startup_setting(suffix: &str) -> Option<std::ffi::OsString> {
    std::env::var_os(format!("KSHELL_{suffix}"))
        .or_else(|| std::env::var_os(format!("MYSH_{suffix}")))
}
