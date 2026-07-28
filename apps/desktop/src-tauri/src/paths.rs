use crate::error::{CodexxError, Result};
#[cfg(test)]
use chrono::Local;
use std::path::{Path, PathBuf};

#[cfg(not(test))]
const APP_HOME_ENV: &str = "EVERYTHING_PATCH_HOME";
#[cfg(not(test))]
const LEGACY_APP_HOME_ENV: &str = "CODEXX_HOME";
const APP_HOME_DIR: &str = ".everything-patch";
const LEGACY_APP_HOME_DIR: &str = ".codexx";

pub(crate) fn home_dir() -> Result<PathBuf> {
    dirs::home_dir().ok_or(CodexxError::NoHomeDir)
}

#[cfg(not(test))]
fn app_home_from_env(name: &str) -> Option<PathBuf> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn resolve_app_home(
    home: &Path,
    current_env: Option<PathBuf>,
    legacy_env: Option<PathBuf>,
) -> PathBuf {
    if let Some(path) = current_env {
        return path;
    }
    if let Some(path) = legacy_env {
        return path;
    }
    let current = home.join(APP_HOME_DIR);
    let legacy = home.join(LEGACY_APP_HOME_DIR);
    if !current.exists() && legacy.is_dir() {
        return legacy;
    }
    current
}

pub(crate) fn app_home() -> Result<PathBuf> {
    #[cfg(test)]
    {
        use std::sync::OnceLock;
        static TEST_APP_HOME: OnceLock<PathBuf> = OnceLock::new();
        Ok(TEST_APP_HOME
            .get_or_init(|| {
                std::env::temp_dir().join(format!(
                    "everything-patch-test-home-{}-{}",
                    std::process::id(),
                    Local::now().timestamp_nanos_opt().unwrap_or_default()
                ))
            })
            .clone())
    }
    #[cfg(not(test))]
    {
        let home = home_dir()?;
        Ok(resolve_app_home(
            &home,
            app_home_from_env(APP_HOME_ENV),
            app_home_from_env(LEGACY_APP_HOME_ENV),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn test_home(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "everything-patch-paths-{label}-{}-{}",
            std::process::id(),
            Local::now().timestamp_nanos_opt().unwrap_or_default()
        ))
    }

    #[test]
    fn current_app_home_env_takes_precedence() {
        let home = test_home("current-env");
        let current = home.join("current-env-home");
        let legacy = home.join("legacy-env-home");

        assert_eq!(
            resolve_app_home(&home, Some(current.clone()), Some(legacy)),
            current
        );
    }

    #[test]
    fn legacy_app_home_env_is_still_supported() {
        let home = test_home("legacy-env");
        let legacy = home.join("legacy-env-home");

        assert_eq!(resolve_app_home(&home, None, Some(legacy.clone())), legacy);
    }

    #[test]
    fn legacy_directory_is_only_used_when_current_directory_is_absent() {
        let home = test_home("legacy-directory");
        let current = home.join(APP_HOME_DIR);
        let legacy = home.join(LEGACY_APP_HOME_DIR);
        fs::create_dir_all(&legacy).expect("create legacy app home");

        assert_eq!(resolve_app_home(&home, None, None), legacy);

        fs::create_dir_all(&current).expect("create current app home");
        assert_eq!(resolve_app_home(&home, None, None), current);

        fs::remove_dir_all(home).expect("remove test home");
    }

    #[test]
    fn fresh_install_uses_current_directory() {
        let home = test_home("fresh-install");

        assert_eq!(resolve_app_home(&home, None, None), home.join(APP_HOME_DIR));
    }
}
