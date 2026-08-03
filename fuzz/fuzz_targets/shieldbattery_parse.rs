#![no_main]

use broodrep::shieldbattery::parse_shieldbattery_section;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = parse_shieldbattery_section(data);
});
