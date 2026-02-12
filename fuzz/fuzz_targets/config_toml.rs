#![no_main]

use libfuzzer_sys::fuzz_target;
use semverguard_types::SemverguardConfig;

fuzz_target!(|data: &[u8]| {
    if let Ok(input) = std::str::from_utf8(data) {
        let _ = toml::from_str::<SemverguardConfig>(input);
    }
});
