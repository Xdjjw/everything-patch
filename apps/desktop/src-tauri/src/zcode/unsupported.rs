//! 非 macOS/Windows 平台的 stub 实现。

use crate::error::{CodexxError, Result};
use std::path::{Path, PathBuf};

pub(crate) fn discover_zcode_app() -> Result<PathBuf> {
    Err(CodexxError::Config(
        "当前平台不支持 ZCode 指令管理（仅支持 macOS 和 Windows）".to_string(),
    ))
}

pub(crate) fn detect_zcode_version(_app_root: &Path) -> Option<String> {
    None
}
