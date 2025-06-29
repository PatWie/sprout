use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use tracing::{debug, info};

use crate::ast::{SproutManifest, PrettyPrint};
use crate::parser::parse_manifest;

/// Load and parse manifest.sprout
pub fn load_manifest(sprout_path: &str) -> Result<SproutManifest> {
    let manifest_path = Path::new(sprout_path).join("manifest.sprout");

    debug!("Loading manifest from: {}", manifest_path.display());

    if !manifest_path.exists() {
        info!("Manifest file does not exist, returning empty manifest");
        return Ok(SproutManifest {
            modules: Vec::new(),
            environments: None,
        });
    }

    let content = fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read manifest: {}", manifest_path.display()))?;

    debug!("Manifest content length: {} bytes", content.len());
    debug!("Manifest content:\n{}", content);

    let manifest = parse_manifest(&content).with_context(|| "Failed to parse manifest.sprout")?;

    info!(
        "Successfully loaded manifest with {} modules",
        manifest.modules.len()
    );
    for package in &manifest.modules {
        debug!("Found package: {}", package.id());
    }

    if let Some(ref environments) = manifest.environments {
        info!("Found {} environments", environments.environments.len());
        for (name, modules) in &environments.environments {
            debug!("Environment '{}' has {} modules", name, modules.len());
        }
    }

    // Validate manifest
    validate_manifest(&manifest)?;

    Ok(manifest)
}

/// Validate manifest for correctness
fn validate_manifest(manifest: &SproutManifest) -> Result<()> {
    use std::collections::HashSet;

    // Check for duplicate package IDs
    let mut seen = HashSet::new();
    for pkg in &manifest.modules {
        let id = pkg.id();
        if !seen.insert(id.clone()) {
            return Err(anyhow::anyhow!("Duplicate package ID: {}", id));
        }
    }

    // Validate dependencies
    for pkg in &manifest.modules {
        for dep in &pkg.depends_on {
            // Check existence
            let dep_exists = manifest.modules.iter().any(|p| p.id() == *dep);
            if !dep_exists {
                return Err(anyhow::anyhow!(
                    "Dependency '{}' not found for package {}",
                    dep,
                    pkg.id()
                ));
            }
        }
    }

    Ok(())
}

/// Save manifest to manifest.sprout (for programmatic modifications)
pub fn save_manifest(sprout_path: &str, manifest: &SproutManifest) -> Result<()> {
    let manifest_path = Path::new(sprout_path).join("manifest.sprout");
    let content = manifest.pretty_print();

    fs::write(&manifest_path, content)
        .with_context(|| format!("Failed to write manifest: {}", manifest_path.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn create_test_manifest() -> SproutManifest {
        let mut exports = HashMap::new();
        exports.insert("PATH".to_string(), vec!["/bin".to_string()]);

        let dep_module = ModuleBlock {
            name: "dep1".to_string(),
            depends_on: vec![],
            exports: vec![],
            fetch: None,
            build: None,
            update: None,
        };

        let module = ModuleBlock {
            name: "test".to_string(),
            depends_on: vec!["dep1".to_string()],
            exports: exports.into_iter().flat_map(|(k, vs)| vs.into_iter().map(move |v| (k.clone(), v))).collect(),
            fetch: Some(FetchBlock {
                spec: FetchSpec::Git(GitSpec {
                    url: "https://example.com/repo.git".to_string(),
                    ref_: Some("v1.0".to_string()),
                    recursive: false,
                }),
            }),
            build: Some(ScriptBlock {
                env: vec![("CC".to_string(), "gcc".to_string())],
                commands: vec!["make".to_string()],
            }),
            update: None,
        };

        let mut environments = HashMap::new();
        environments.insert("dev".to_string(), vec!["test".to_string()]);

        SproutManifest {
            modules: vec![dep_module, module],
            environments: Some(EnvironmentsBlock { environments }),
        }
    }

    #[test]
    fn test_serialize_manifest() {
        let manifest = create_test_manifest();
        let serialized = manifest.pretty_print();

        eprintln!("Serialized:\n{}", serialized);
        assert!(serialized.contains("module test"));
        assert!(serialized.contains("depends_on = [dep1]"));
        assert!(serialized.contains("git = {"));
        assert!(serialized.contains("url = https://example.com/repo.git"));
        assert!(serialized.contains("environments {"));
        assert!(serialized.contains("dev = ["));
    }

    #[test]
    fn test_save_load_manifest() {
        let temp_dir = TempDir::new().unwrap();
        let sprout_path = temp_dir.path().to_str().unwrap();

        let manifest = create_test_manifest();

        // Save
        save_manifest(sprout_path, &manifest).unwrap();

        // Load
        let loaded_manifest = load_manifest(sprout_path).unwrap();
        assert_eq!(loaded_manifest.modules.len(), 2);
        assert_eq!(loaded_manifest.modules[1].name, "test");
        assert!(loaded_manifest.environments.is_some());
    }

    #[test]
    fn test_load_missing_manifest() {
        let temp_dir = TempDir::new().unwrap();
        let sprout_path = temp_dir.path().to_str().unwrap();

        // Loading non-existent manifest should return empty
        let manifest = load_manifest(sprout_path).unwrap();
        assert!(manifest.modules.is_empty());
        assert!(manifest.environments.is_none());
    }
}
