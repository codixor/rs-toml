//! Parse arbitrary input; it must never panic. When parsing succeeds, emit the
//! document back to TOML and re-parse it: the emitted text must be accepted
//! and describe the same value tree.
#![no_main]

use libfuzzer_sys::fuzz_target;
use rs_toml::{Arena, Category, FormatError, Formatting, Limits};
use rs_toml_fuzz::{from_rs_toml, from_rs_toml_table};

fuzz_target!(|data: &[u8]| {
    let Ok(input) = std::str::from_utf8(data) else {
        return;
    };
    let arena = Arena::new();
    let Ok(doc) = rs_toml::parse(input, &arena) else {
        return;
    };
    let original = from_rs_toml_table(doc.table());
    // A valid document under stricter bounds either keeps its tree or reports
    // the first exceeded bound. Mutate depth, document and token independently.
    if let [depth, doc_bytes, token, ..] = data {
        let limits = Limits {
            max_depth: u32::from(*depth % 16),
            max_document_bytes: u64::from(*doc_bytes),
            max_token_bytes: usize::from(*token % 32),
            max_expansion: 0,
        };
        let bounded_arena = Arena::new();
        match rs_toml::parse_with(input, &bounded_arena, &limits) {
            Ok(bounded) => assert_eq!(from_rs_toml_table(bounded.table()), original),
            Err(error) => {
                assert_eq!(error.category(), Category::Limit);
                let offset = error.position().unwrap().offset.unwrap();
                assert!(offset <= input.len() as u64);
            }
        }
    }
    let table = doc.into_table();

    let emitted = Formatting::default().format_table_to_bytes(table, &arena);
    let emitted = std::str::from_utf8(&emitted).expect("emitter produced invalid UTF-8");

    let arena2 = Arena::new();
    let reparsed = match rs_toml::parse(emitted, &arena2) {
        Ok(doc) => doc,
        Err(err) => panic!(
            "emitted TOML does not re-parse: {err:?}\n--- input\n{input}\n--- emitted\n{emitted}"
        ),
    };
    let roundtrip = from_rs_toml(&reparsed.into_item());
    assert!(
        original == roundtrip,
        "round-trip changed the value\n--- input\n{input}\n--- emitted\n{emitted}\n--- before\n{original:?}\n--- after\n{roundtrip:?}"
    );
});
