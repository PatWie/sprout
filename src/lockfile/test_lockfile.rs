use crate::lockfile::{SproutLock, PackageState};
use tempfile::TempDir;

#[test]
fn test_lockfile_operations() {
    let temp_dir = TempDir::new().unwrap();
    let sprout_path = temp_dir.path().to_str().unwrap();
    
    let mut lock = SproutLock::load(sprout_path).unwrap();
    
    // Test package operations
    assert!(lock.get_module_state("test@1.0").is_none());
    lock.set_module_state("test@1.0".to_string(), PackageState {
        fetch_hash: None,
        build_hash: Some("hash123".to_string()),
    });
    assert_eq!(lock.get_module_state("test@1.0").unwrap().build_hash, Some("hash123".to_string()));
    
    // lock.remove_module("test@1.0");
    // assert!(lock.get_module_state("test@1.0").is_none());
}

#[test]
fn test_lockfile_save_load() {
    let temp_dir = TempDir::new().unwrap();
    let sprout_path = temp_dir.path().to_str().unwrap();
    
    let mut lock = SproutLock::load(sprout_path).unwrap();
    lock.set_module_state("test@1.0".to_string(), PackageState {
        fetch_hash: None,
        build_hash: Some("hash123".to_string()),
    });
    lock.symlinks.insert(".zshrc".to_string(), "symlink_hash".to_string());
    
    // Save
    lock.save(sprout_path).unwrap();
    
    // Load
    let loaded_lock = SproutLock::load(sprout_path).unwrap();
    assert_eq!(loaded_lock.get_module_state("test@1.0").unwrap().build_hash, Some("hash123".to_string()));
    assert_eq!(loaded_lock.symlinks.get(".zshrc"), Some(&"symlink_hash".to_string()));
}

#[test]
fn test_lockfile_load_missing() {
    let temp_dir = TempDir::new().unwrap();
    let sprout_path = temp_dir.path().to_str().unwrap();
    
    // Loading non-existent lockfile should return default
    let lock = SproutLock::load(sprout_path).unwrap();
    assert!(lock.modules.is_empty());
    assert!(lock.symlinks.is_empty());
}
