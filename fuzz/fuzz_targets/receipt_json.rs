#![no_main]

use libfuzzer_sys::fuzz_target;
use semverguard_types::SensorReportV1;

fuzz_target!(|data: &[u8]| {
    if let Ok(input) = std::str::from_utf8(data) {
        let _ = serde_json::from_str::<SensorReportV1>(input);
    }
});
