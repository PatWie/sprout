#[cfg(test)]
mod tests {
    use crate::core::init_sprout;
    use crate::manifest::{load_manifest, save_manifest};
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_init_creates_empty_manifest() {
        let temp_dir = TempDir::new().unwrap();
        let sprout_path = temp_dir.path().to_str().unwrap();

        init_sprout(sprout_path, true).unwrap();

        // Create empty manifest since init doesn't create one
        let manifest = load_manifest(sprout_path).unwrap();
        save_manifest(sprout_path, &manifest).unwrap();

        let manifest_content =
            fs::read_to_string(format!("{}/manifest.sprout", sprout_path)).unwrap();
        insta::assert_snapshot!(manifest_content);
    }
}
