#[cfg(test)]
mod tests {
    use crate::parser::parse_manifest;
    use crate::ast::PrettyPrint;
    use std::fs;
    use std::path::Path;

    fn read_snapshots_dir() -> Vec<(String, String)> {
        let snapshots_dir = Path::new("src/snapshots");
        let mut snapshots = Vec::new();

        if !snapshots_dir.exists() {
            return snapshots;
        }

        for entry in fs::read_dir(snapshots_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();

            if path.extension().map_or(false, |ext| ext == "snap") {
                let content = fs::read_to_string(&path).unwrap();
                let filename = path.file_name().unwrap().to_string_lossy().to_string();

                // Split on first --- then take everything after the second ---
                let parts: Vec<&str> = content.split("---").collect();
                if parts.len() >= 3 {
                    let manifest_content = parts[2].trim();
                    // Skip symlink test snapshots (they contain directory structure, not manifests)
                    if !manifest_content.starts_with("=== symlinks/") {
                        snapshots.push((filename, manifest_content.to_string()));
                    }
                }
            }
        }

        snapshots
    }

    #[test]
    fn test_parse_all_snapshots() {
        let snapshots = read_snapshots_dir();

        assert!(!snapshots.is_empty(), "No snapshots found to test");

        for (filename, manifest_content) in snapshots {
            println!("Testing snapshot: {}", filename);

            if manifest_content.is_empty() {
                println!("  Skipping empty manifest");
                continue;
            }

            match parse_manifest(&manifest_content) {
                Ok(parsed_manifest) => {
                    println!("  ✓ Parsed {} modules", parsed_manifest.modules.len());

                    // Verify each package has required fields
                    for package in &parsed_manifest.modules {
                        assert!(!package.name.is_empty(), "Package name empty in {}", filename);
                    }
                }
                Err(e) => {
                    panic!("Failed to parse snapshot {}: {}", filename, e);
                }
            }
        }
    }

    #[test]
    fn test_roundtrip_all_snapshots() {
        let snapshots = read_snapshots_dir();

        for (filename, manifest_content) in snapshots {
            if manifest_content.is_empty() {
                continue; // Skip empty manifests
            }

            println!("Testing roundtrip for: {}", filename);

            // Parse the manifest
            let parsed = parse_manifest(&manifest_content).unwrap_or_else(|e| {
                panic!("Failed to parse {}: {}", filename, e);
            });

            // Serialize it back
            let serialized = parsed.pretty_print();

            // Parse the serialized version
            let reparsed = parse_manifest(&serialized).unwrap_or_else(|e| {
                panic!("Failed to reparse serialized version of {}: {}", filename, e);
            });

            // Compare key properties
            assert_eq!(parsed.modules.len(), reparsed.modules.len(),
                      "Package count mismatch in roundtrip for {}", filename);

            for (orig, reparsed_pkg) in parsed.modules.iter().zip(reparsed.modules.iter()) {
                assert_eq!(orig.name, reparsed_pkg.name, "Name mismatch in {}", filename);
            }

            println!("  ✓ Roundtrip successful");
        }
    }
}
