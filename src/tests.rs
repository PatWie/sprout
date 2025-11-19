#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::*;
    use crate::ast::PrettyPrint;
    use crate::parser::parse_manifest;
    use std::collections::HashMap;

    fn create_test_git_package() -> ModuleBlock {
        let mut exports = HashMap::new();
        exports.insert("PATH".to_string(), vec!["/bin".to_string()]);

        ModuleBlock {
            name: "fd".to_string(),
            depends_on: vec![],
            exports: exports.into_iter().flat_map(|(k, vs)| vs.into_iter().map(move |v| (k.clone(), v))).collect(),
            fetch: Some(FetchBlock {
                spec: FetchSpec::Git(GitSpec {
                    url: "https://github.com/sharkdp/fd.git".to_string(),
                    ref_: Some("v8.7.0".to_string()),
                    recursive: false,
                }),
                output: None,
            }),
            build: Some(ScriptBlock {
                env: vec![],
                commands: vec![
                    "make".to_string(),
                    "make install PREFIX=${DIST_PATH}".to_string(),
                ],
            }),
            update: None,
        }
    }

    fn create_test_cargo_package() -> ModuleBlock {
        let mut exports = HashMap::new();
        exports.insert("PATH".to_string(), vec!["/bin".to_string()]);

        ModuleBlock {
            name: "bat".to_string(),
            depends_on: vec![],
            exports: exports.into_iter().flat_map(|(k, vs)| vs.into_iter().map(move |v| (k.clone(), v))).collect(),
            fetch: None, // Cargo modules don't need fetch
            build: Some(ScriptBlock {
                env: vec![],
                commands: vec![
                    "cargo install bat --version 0.24.0 --root ${DIST_PATH}".to_string(),
                ],
            }),
            update: None,
        }
    }

    fn create_test_tar_package() -> ModuleBlock {
        let mut exports = HashMap::new();
        exports.insert("PATH".to_string(), vec!["/bin".to_string()]);

        ModuleBlock {
            name: "hello".to_string(),
            depends_on: vec![],
            exports: exports.into_iter().flat_map(|(k, vs)| vs.into_iter().map(move |v| (k.clone(), v))).collect(),
            fetch: Some(FetchBlock {
                spec: FetchSpec::Http(HttpSpec {
                    url: "https://ftp.gnu.org/gnu/hello/hello-2.12.tar.gz".to_string(),
                    sha256: None,
                }),
                output: None,
            }),
            build: Some(ScriptBlock {
                env: vec![],
                commands: vec![
                    "make".to_string(),
                    "make install PREFIX=${DIST_PATH}".to_string(),
                ],
            }),
            update: None,
        }
    }

    #[test]
    fn test_git_module_serialization() {
        let package = create_test_git_package();
        let manifest = SproutManifest {
            modules: vec![package],
            environments: None,
        };

        let serialized = manifest.pretty_print();
        
        let expected = r#"module fd {
    depends_on = []
    exports = {
        PATH = "/bin"
    }
    fetch {
        git = {
            url = https://github.com/sharkdp/fd.git
            ref = v8.7.0
        }
    }
    build {
        make
        make install PREFIX=${DIST_PATH}
    }
}

"#;
        assert_eq!(serialized, expected);
    }

    #[test]
    fn test_cargo_module_serialization() {
        let package = create_test_cargo_package();
        let manifest = SproutManifest {
            modules: vec![package],
            environments: None,
        };

        let serialized = manifest.pretty_print();
        
        let expected = r#"module bat {
    depends_on = []
    exports = {
        PATH = "/bin"
    }
    build {
        cargo install bat --version 0.24.0 --root ${DIST_PATH}
    }
}

"#;
        assert_eq!(serialized, expected);
    }

    #[test]
    fn test_tar_module_serialization() {
        let package = create_test_tar_package();
        let manifest = SproutManifest {
            modules: vec![package],
            environments: None,
        };

        let serialized = manifest.pretty_print();
        
        let expected = r#"module hello {
    depends_on = []
    exports = {
        PATH = "/bin"
    }
    fetch {
        http = {
            url = https://ftp.gnu.org/gnu/hello/hello-2.12.tar.gz
        }
    }
    build {
        make
        make install PREFIX=${DIST_PATH}
    }
}

"#;
        assert_eq!(serialized, expected);
    }

    #[test]
    fn test_git_module_round_trip() {
        let original_package = create_test_git_package();
        let manifest = SproutManifest {
            modules: vec![original_package.clone()],
            environments: None,
        };

        // Serialize
        let serialized = manifest.pretty_print();
        println!("Serialized:\n{}", serialized);

        // Deserialize
        let parsed_manifest =
            parse_manifest(&serialized).expect("Failed to parse serialized manifest");

        // Compare
        assert_eq!(parsed_manifest.modules.len(), 1);
        let parsed_package = &parsed_manifest.modules[0];

        assert_eq!(parsed_package.name, original_package.name);
        assert_eq!(parsed_package.depends_on, original_package.depends_on);
        assert_eq!(parsed_package.exports, original_package.exports);

        // Check fetch block
        assert!(parsed_package.fetch.is_some());
        assert!(original_package.fetch.is_some());

        match (
            &parsed_package.fetch.as_ref().unwrap().spec,
            &original_package.fetch.as_ref().unwrap().spec,
        ) {
            (FetchSpec::Git(parsed_git), FetchSpec::Git(original_git)) => {
                assert_eq!(parsed_git.url, original_git.url);
                assert_eq!(parsed_git.ref_, original_git.ref_);
            }
            _ => panic!("Fetch spec type mismatch"),
        }

        // Check build and install blocks
        assert!(parsed_package.build.is_some());
        assert_eq!(
            parsed_package.build.as_ref().unwrap().commands,
            original_package.build.as_ref().unwrap().commands
        );
    }

    #[test]
    fn test_cargo_module_round_trip() {
        let original_package = create_test_cargo_package();
        let manifest = SproutManifest {
            modules: vec![original_package.clone()],
            environments: None,
        };

        // Serialize
        let serialized = manifest.pretty_print();
        println!("Serialized:\n{}", serialized);

        // Deserialize
        let parsed_manifest =
            parse_manifest(&serialized).expect("Failed to parse serialized manifest");

        // Compare
        assert_eq!(parsed_manifest.modules.len(), 1);
        let parsed_package = &parsed_manifest.modules[0];

        assert_eq!(parsed_package.name, original_package.name);
        assert_eq!(parsed_package.depends_on, original_package.depends_on);
        assert_eq!(parsed_package.exports, original_package.exports);

        // Cargo modules should not have fetch block
        assert!(parsed_package.fetch.is_none());
        assert!(original_package.fetch.is_none());

        // Check build block
        assert!(parsed_package.build.is_some());
        assert_eq!(
            parsed_package.build.as_ref().unwrap().commands,
            original_package.build.as_ref().unwrap().commands
        );

    }

    #[test]
    fn test_tar_module_round_trip() {
        let original_package = create_test_tar_package();
        let manifest = SproutManifest {
            modules: vec![original_package.clone()],
            environments: None,
        };

        // Serialize
        let serialized = manifest.pretty_print();
        println!("Serialized:\n{}", serialized);

        // Deserialize
        let parsed_manifest =
            parse_manifest(&serialized).expect("Failed to parse serialized manifest");

        // Compare
        assert_eq!(parsed_manifest.modules.len(), 1);
        let parsed_package = &parsed_manifest.modules[0];

        assert_eq!(parsed_package.name, original_package.name);
        assert_eq!(parsed_package.depends_on, original_package.depends_on);
        assert_eq!(parsed_package.exports, original_package.exports);

        // Check fetch block
        assert!(parsed_package.fetch.is_some());
        assert!(original_package.fetch.is_some());

        match (
            &parsed_package.fetch.as_ref().unwrap().spec,
            &original_package.fetch.as_ref().unwrap().spec,
        ) {
            (FetchSpec::Http(parsed_tar), FetchSpec::Http(original_tar)) => {
                assert_eq!(parsed_tar.url, original_tar.url);
                assert_eq!(parsed_tar.sha256, original_tar.sha256);
            }
            _ => panic!("Fetch spec type mismatch"),
        }

        // Check build and install blocks
        assert!(parsed_package.build.is_some());
        assert_eq!(
            parsed_package.build.as_ref().unwrap().commands,
            original_package.build.as_ref().unwrap().commands
        );
    }

    #[test]
    fn test_mixed_modules_manifest() {
        let git_package = create_test_git_package();
        let cargo_package = create_test_cargo_package();
        let tar_package = create_test_tar_package();

        let manifest = SproutManifest {
            modules: vec![git_package, cargo_package, tar_package],
            environments: None,
        };

        // Serialize
        let serialized = manifest.pretty_print();
        println!("Mixed manifest serialized:\n{}", serialized);

        // Deserialize
        let parsed_manifest = parse_manifest(&serialized).expect("Failed to parse mixed manifest");

        // Check all modules are present
        assert_eq!(parsed_manifest.modules.len(), 3);

        // Find modules by name (they might be sorted)
        let mut found_git = false;
        let mut found_cargo = false;
        let mut found_tar = false;

        for package in &parsed_manifest.modules {
            match package.name.as_str() {
                "fd" => {
                    found_git = true;
                    assert!(package.fetch.is_some());
                    assert!(package.build.is_some());
                }
                "bat" => {
                    found_cargo = true;
                    assert!(package.fetch.is_none());
                    assert!(package.build.is_some());
                }
                "hello" => {
                    found_tar = true;
                    assert!(package.fetch.is_some());
                    assert!(package.build.is_some());
                }
                _ => panic!("Unexpected package name: {}", package.name),
            }
        }

        assert!(found_git, "Git package not found");
        assert!(found_cargo, "Cargo package not found");
        assert!(found_tar, "Tar package not found");
    }

    #[test]
    fn test_environments_serialization() {
        let git_package = create_test_git_package();
        let cargo_package = create_test_cargo_package();

        let mut environments = HashMap::new();
        environments.insert(
            "dev".to_string(),
            vec!["fd@v8.7.0".to_string(), "bat@0.24.0".to_string()],
        );
        environments.insert("minimal".to_string(), vec!["fd@v8.7.0".to_string()]);

        let manifest = SproutManifest {
            modules: vec![git_package, cargo_package],
            environments: Some(EnvironmentsBlock { environments }),
        };

        // Serialize
        let serialized = manifest.pretty_print();
        
        let expected = r#"module bat {
    depends_on = []
    exports = {
        PATH = "/bin"
    }
    build {
        cargo install bat --version 0.24.0 --root ${DIST_PATH}
    }
}

module fd {
    depends_on = []
    exports = {
        PATH = "/bin"
    }
    fetch {
        git = {
            url = https://github.com/sharkdp/fd.git
            ref = v8.7.0
        }
    }
    build {
        make
        make install PREFIX=${DIST_PATH}
    }
}

environments {
    dev = [fd@v8.7.0, bat@0.24.0]

    minimal = [fd@v8.7.0]

}
"#;
        assert_eq!(serialized, expected);

        // Deserialize and verify
        let parsed_manifest =
            parse_manifest(&serialized).expect("Failed to parse manifest with environments");

        assert_eq!(parsed_manifest.modules.len(), 2);
        assert!(parsed_manifest.environments.is_some());

        let parsed_envs = parsed_manifest.environments.as_ref().unwrap();
        assert_eq!(parsed_envs.environments.len(), 2);
        assert!(parsed_envs.environments.contains_key("dev"));
        assert!(parsed_envs.environments.contains_key("minimal"));
    }
}
