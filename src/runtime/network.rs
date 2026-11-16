//! The synchronous dispatcher shares one lazily initialized networking runtime.
use crate::error::ShellError;
use std::sync::OnceLock;
static RUNTIME: OnceLock<Result<tokio::runtime::Runtime, String>> = OnceLock::new();
pub(crate) fn runtime() -> Result<&'static tokio::runtime::Runtime, ShellError> {
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| ShellError::new(format!("network runtime: {e}"), 1))
}
