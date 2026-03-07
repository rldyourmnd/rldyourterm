#![no_main]

use libfuzzer_sys::fuzz_target;
use rldyourterm_core::parser::Parser;

fuzz_target!(|data: &[u8]| {
    let mut parser = Parser::default();
    let mut actions = Vec::new();

    for chunk in data.chunks(64) {
        parser.feed_into(chunk, &mut actions);
    }

    let _ = parser.resync_after_truncation();
});
