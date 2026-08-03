#![no_main]

use broodrep::TextEncoding;
use broodrep::commands::parse_commands;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Both encodings are exercised on every input: Utf8 is what modern replays use, while
    // Legacy runs the more complex utf-8 -> cp949 -> windows-1252 fallback chain when decoding
    // chat messages.
    let _ = parse_commands(data, TextEncoding::Utf8);
    let _ = parse_commands(data, TextEncoding::Legacy);
});
