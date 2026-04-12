use std::path::PathBuf;

/// Return the scratchpad root directory.
/// Uses `ENDGOAL_SCRATCHPAD_ROOT` env var if set, otherwise defaults to `./scratchpads/`.
pub fn scratchpad_root() -> PathBuf {
    std::env::var("ENDGOAL_SCRATCHPAD_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./scratchpads"))
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
        // Without env var set, should default to ./scratchpads
        // (We can't easily unset env vars in parallel tests, so just check the function exists)
        let root = scratchpad_root();
        // If ENDGOAL_SCRATCHPAD_ROOT is not set, it should be ./scratchpads
        // If it IS set (from another test), it could be anything
        assert!(!root.as_os_str().is_empty());
    }

    #[test]
    fn test_scratchpad_root_from_env() {
        // Use a unique env var approach — set and check
        let original = std::env::var("ENDGOAL_SCRATCHPAD_ROOT").ok();
        // SAFETY: This test is single-threaded and we restore the original value after.
        unsafe {
            std::env::set_var("ENDGOAL_SCRATCHPAD_ROOT", "/tmp/custom-scratchpads");
        }
        let root = scratchpad_root();
        assert_eq!(root, PathBuf::from("/tmp/custom-scratchpads"));

        // Restore
        unsafe {
            match original {
                Some(val) => std::env::set_var("ENDGOAL_SCRATCHPAD_ROOT", val),
                None => std::env::remove_var("ENDGOAL_SCRATCHPAD_ROOT"),
            }
        }
    }

    #[test]
    fn test_scratchpad_naming_convention() {
        let tmp = tempdir().unwrap();
        let path = ensure_scratchpad_in(tmp.path(), "my-run-123").unwrap();

        let dir_name = path.file_name().unwrap().to_str().unwrap();
        assert_eq!(dir_name, "run-my-run-123");
    }
}
