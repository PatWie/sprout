#[cfg(test)]
mod tests {
    use crate::core::{init_sprout, add_file};
    use tempfile::TempDir;
    use std::fs;
    use std::path::Path;

    fn create_test_files(temp_dir: &Path) -> Vec<String> {
        let home_dir = temp_dir.join("home");
        fs::create_dir_all(&home_dir).unwrap();
        
        // Create test files
        let files = vec![
            ".bashrc",
            ".zshrc", 
            ".gitconfig",
            ".config/nvim/init.vim",
            ".config/alacritty/alacritty.yml",
        ];
        
        let mut created_files = Vec::new();
        
        for file in files {
            let file_path = home_dir.join(file);
            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&file_path, format!("# Test content for {}", file)).unwrap();
            created_files.push(file_path.to_string_lossy().to_string());
        }
        
        created_files
    }

    fn capture_symlink_state(sprout_path: &str) -> String {
        let mut output = String::new();
        
        // Read .sproutindex if it exists
        let index_path = format!("{}/.sproutindex", sprout_path);
        if Path::new(&index_path).exists() {
            let index_content = fs::read_to_string(&index_path).unwrap();
            output.push_str("=== .sproutindex ===\n");
            output.push_str(&index_content);
            output.push_str("\n");
        }
        
        // List symlinks directory structure
        let symlinks_dir = format!("{}/symlinks", sprout_path);
        if Path::new(&symlinks_dir).exists() {
            output.push_str("=== symlinks/ structure ===\n");
            output.push_str(&list_directory_tree(&symlinks_dir, ""));
        }
        
        output
    }

    fn list_directory_tree(dir: &str, prefix: &str) -> String {
        let mut output = String::new();
        
        if let Ok(entries) = fs::read_dir(dir) {
            let mut entries: Vec<_> = entries.collect();
            entries.sort_by_key(|e| e.as_ref().unwrap().file_name());
            
            for entry in entries {
                let entry = entry.unwrap();
                let name = entry.file_name().to_string_lossy().to_string();
                let path = entry.path();
                
                if path.is_dir() {
                    output.push_str(&format!("{}📁 {}/\n", prefix, name));
                    output.push_str(&list_directory_tree(&path.to_string_lossy(), &format!("{}  ", prefix)));
                } else {
                    output.push_str(&format!("{}📄 {}\n", prefix, name));
                }
            }
        }
        
        output
    }

    #[test]
    fn test_add_single_file() {
        let temp_dir = TempDir::new().unwrap();
        let sprout_path = temp_dir.path().join("sprout").to_string_lossy().to_string();
        let tracking_path = temp_dir.path().join("home").to_string_lossy().to_string();
        
        let _files = create_test_files(temp_dir.path());
        init_sprout(&sprout_path, false).unwrap();
        
        // Add single file using full path
        let bashrc_path = temp_dir.path().join("home/.bashrc");
        add_file(&sprout_path, bashrc_path, false, false, &tracking_path).unwrap();
        
        let state = capture_symlink_state(&sprout_path);
        insta::assert_snapshot!(state);
    }

    #[test]
    fn test_add_recursive_directory() {
        let temp_dir = TempDir::new().unwrap();
        let sprout_path = temp_dir.path().join("sprout").to_string_lossy().to_string();
        let tracking_path = temp_dir.path().join("home").to_string_lossy().to_string();
        
        let _files = create_test_files(temp_dir.path());
        init_sprout(&sprout_path, false).unwrap();
        
        // Add .config directory recursively using full path
        let config_path = temp_dir.path().join("home/.config");
        add_file(&sprout_path, config_path, true, false, &tracking_path).unwrap();
        
        let state = capture_symlink_state(&sprout_path);
        insta::assert_snapshot!(state);
    }

    #[test]
    fn test_dry_run_mode() {
        let temp_dir = TempDir::new().unwrap();
        let sprout_path = temp_dir.path().join("sprout").to_string_lossy().to_string();
        let tracking_path = temp_dir.path().join("home").to_string_lossy().to_string();
        
        let _files = create_test_files(temp_dir.path());
        init_sprout(&sprout_path, false).unwrap();
        
        // Dry run using full path
        let bashrc_path = temp_dir.path().join("home/.bashrc");
        add_file(&sprout_path, bashrc_path, false, true, &tracking_path).unwrap();
        
        let state = capture_symlink_state(&sprout_path);
        insta::assert_snapshot!(state);
    }
}
