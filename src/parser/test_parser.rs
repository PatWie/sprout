use super::*;
use crate::ast::FetchSpec;
use pest::Parser;

#[test]
fn test_parse_simple_package() {
    let input = r#"
module clang {
    depends_on = ["gcc"]
    exports = { PATH = "/bin" }

    fetch {
        git = {
            url = https://github.com/llvm/llvm-project.git
            ref = llvmorg-20.1.7
        }
    }

    build {
        env {
            CC = "/sprout/dist/gcc/bin/gcc"
        }
        mkdir -p build
        cd build
        cmake ../llvm
        make -j8
        make install
    }
}
"#;

    let manifest = parse_manifest(input).unwrap();
    assert_eq!(manifest.modules.len(), 1);

    let pkg = &manifest.modules[0];
    assert_eq!(pkg.name, "clang");
    assert_eq!(pkg.depends_on, vec!["gcc"]);
    assert!(pkg.fetch.is_some());
    assert!(pkg.build.is_some());

    // Validate env block
    let build = pkg.build.as_ref().unwrap();
    assert_eq!(build.env.len(), 1);
    assert_eq!(
        build.env[0],
        (
            "CC".to_string(),
            "/sprout/dist/gcc/bin/gcc".to_string()
        )
    );
    assert_eq!(build.commands.len(), 5);
}

#[test]
fn test_parse_environments() {
    let input = r#"
environments {
    dev = [
        ripgrep@14.1.1,
        tmux@default
    ]

    modern = [
        gcc@default,
        clang@default
    ]
}
"#;

    let manifest = parse_manifest(input).unwrap();
    assert!(manifest.environments.is_some());

    let envs = manifest.environments.unwrap();
    assert_eq!(envs.environments.len(), 2);
    assert!(envs.environments.contains_key("dev"));
    assert!(envs.environments.contains_key("modern"));
}

#[test]

#[test]
fn test_parse_manifest_with_archive_spec() {
    let manifest_content = r#"
module example {
    depends_on = []
    fetch {
        http = {
            url = https://example.com/file.tar.gz
            sha256 = abc123def456
        }
    }
    build {
        ./configure --prefix=${DIST_PATH}
        make
        make install
    }
}
"#;

    let result = parse_manifest(manifest_content);
    assert!(
        result.is_ok(),
        "Failed to parse manifest with http spec: {:?}",
        result.err()
    );

    let manifest = result.unwrap();
    assert_eq!(manifest.modules.len(), 1);

    let package = &manifest.modules[0];
    assert_eq!(package.name, "example");

    assert!(package.fetch.is_some());
    match &package.fetch.as_ref().unwrap().spec {
        FetchSpec::Http(http_spec) => {
            assert_eq!(http_spec.url, "https://example.com/file.tar.gz");
            assert_eq!(http_spec.sha256, Some("abc123def456".to_string()));
        }
        _ => panic!("Expected http fetch spec"),
    }
}

#[test]
fn test_parse_manifest_with_local_spec() {
    let manifest_content = r#"
module localproject {
    depends_on = []
    fetch {
        local = {
            path = /path/to/local/project
        }
    }
    build {
        ./configure --prefix=${DIST_PATH}
        make
        make install
    }
}
"#;

    let result = parse_manifest(manifest_content);
    assert!(
        result.is_ok(),
        "Failed to parse manifest with local spec: {:?}",
        result.err()
    );

    let manifest = result.unwrap();
    assert_eq!(manifest.modules.len(), 1);

    let package = &manifest.modules[0];
    assert_eq!(package.name, "localproject");

    assert!(package.fetch.is_some());
    match &package.fetch.as_ref().unwrap().spec {
        FetchSpec::Local(local_spec) => {
            assert_eq!(local_spec.path, "/path/to/local/project");
        }
        _ => panic!("Expected local fetch spec"),
    }
}
