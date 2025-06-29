#![no_main]

use libfuzzer_sys::fuzz_target;
use sprout::parser::parse_manifest;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // Just try to parse - don't panic on parse errors
        let _ = parse_manifest(s);
    }
});
