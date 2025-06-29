#[cfg(test)]
mod tests {
    use crate::parser::parse_manifest;

    #[test]
    fn test_parser_with_random_input() {
        let test_cases = vec![
            "",
            "invalid",
            "package",
            "module name",
            "module name",
            "module name {",
            "module name { }",
            "module name { invalid }",
            "module name { fetch }",
            "module name { fetch { } }",
            "module name { fetch { git } }",
            "module name { fetch { git { } } }",
            "module name { fetch { git { url } } }",
            "module name { fetch { git { url = } } }",
            "module name { fetch { git { url = invalid } } }",
            "module name { fetch { git { url = \"\" } } }",
            "module name { fetch { git { url = \"valid\" } } }",
            "module name { fetch { git { url = \"valid\" ref = \"v1.0\" } } }",
            "module name { depends_on = }",
            "module name { depends_on = [ }",
            "module name { depends_on = [] }",
            "module name { depends_on = [\"dep\"] }",
            "module name { build }",
            "module name { build { } }",
            "module name { build { make } }",
            "module name { exports }",
            "module name { exports { } }",
            "module name { exports { PATH } }",
            "module name { exports { PATH = } }",
            "module name { exports { PATH = [] } }",
            "module name { exports { PATH = [\"/bin\"] } }",
        ];

        for input in test_cases {
            // Should not panic, just return Ok or Err
            let result = parse_manifest(input);
            match result {
                Ok(_) => println!("✓ Parsed: {}", input),
                Err(_) => println!("✗ Failed: {}", input),
            }
        }
    }

    #[test]
    fn test_parser_with_malformed_braces() {
        let malformed_cases = vec![
            "module name {",
            "module name { { }",
            "module name { } }",
            "module name { fetch { }",
            "module name { fetch { git { }",
            "module name { fetch { git { url = \"test\" }",
            "module name { fetch { git { url = \"test\" } }",
        ];

        for input in malformed_cases {
            let result = parse_manifest(input);
            // These should all fail gracefully
            assert!(result.is_err(), "Expected parse error for: {}", input);
        }
    }

    #[test]
    fn test_parser_with_unicode_and_special_chars() {
        let unicode_cases = vec![
            "module 测试@1.0 { }",
            "module name { fetch { git { url = \"https://github.com/用户/项目.git\" } } }",
            "module name { build { echo \"Hello 世界\" } }",
            "module name { exports { PATH = [\"/usr/bin/测试\"] } }",
            "module name { depends_on = [\"依赖@1.0\"] }",
        ];

        for input in unicode_cases {
            let result = parse_manifest(input);
            // Should handle unicode gracefully (pass or fail, but not panic)
            match result {
                Ok(_) => println!("✓ Unicode parsed: {}", input),
                Err(_) => println!("✗ Unicode failed: {}", input),
            }
        }
    }
}
