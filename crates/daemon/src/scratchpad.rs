use std::path::PathBuf;

/// Return the scratchpad root directory.
/// Uses `ENDGOAL_SCRATCHPAD_ROOT` env var if set, otherwise defaults to `./scratchpads/`.
pub fn scratchpad_root() -> PathBuf {
    scratchpad_root_from_env(std::env::var("ENDGOAL_SCRATCHPAD_ROOT").ok())
}

fn scratchpad_root_from_env(value: Option<String>) -> PathBuf {
    value
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("./scratchpads"))
}

/// Create and return the scratchpad directory for a given run ID.
/// Creates `$ENDGOAL_SCRATCHPAD_ROOT/run-{id}/`.
pub fn ensure_scratchpad(run_id: &str) -> std::io::Result<PathBuf> {
    let root = scratchpad_root();
    let path = root.join(format!("run-{run_id}"));
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

/// Create and return the scratchpad directory under a specific root.
/// This is the testable version that doesn't rely on env vars.
pub fn ensure_scratchpad_in(root: &std::path::Path, run_id: &str) -> std::io::Result<PathBuf> {
    let path = root.join(format!("run-{run_id}"));
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_ensure_scratchpad_creates_directory() {
        let tmp = tempdir().unwrap();
        let path = ensure_scratchpad_in(tmp.path(), "test-1").unwrap();

        assert!(path.exists());
        assert!(path.is_dir());
        assert_eq!(path, tmp.path().join("run-test-1"));
    }

    #[test]
    fn test_ensure_scratchpad_idempotent() {
        let tmp = tempdir().unwrap();

        let path1 = ensure_scratchpad_in(tmp.path(), "test-2").unwrap();
        let path2 = ensure_scratchpad_in(tmp.path(), "test-2").unwrap();

        assert_eq!(path1, path2);
        assert!(path1.exists());
    }

    #[test]
    fn test_ensure_scratchpad_different_ids() {
        let tmp = tempdir().unwrap();

        let path_a = ensure_scratchpad_in(tmp.path(), "run-a").unwrap();
        let path_b = ensure_scratchpad_in(tmp.path(), "run-b").unwrap();

        assert_ne!(path_a, path_b);
        assert!(path_a.exists());
        assert!(path_b.exists());
    }

    #[test]
    fn test_scratchpad_root_default() {
        let root = scratchpad_root_from_env(None);
        assert_eq!(root, PathBuf::from("./scratchpads"));
    }

    #[test]
    fn test_scratchpad_root_from_env() {
        let root = scratchpad_root_from_env(Some("/tmp/custom-scratchpads".to_string()));
        assert_eq!(root, PathBuf::from("/tmp/custom-scratchpads"));
    }

    #[test]
    fn test_scratchpad_naming_convention() {
        let tmp = tempdir().unwrap();
        let path = ensure_scratchpad_in(tmp.path(), "my-run-123").unwrap();

        let dir_name = path.file_name().unwrap().to_str().unwrap();
        assert_eq!(dir_name, "run-my-run-123");
    }
}
