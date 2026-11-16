//! Differential target: rs-toml and the `toml` crate (both TOML 1.1) must
//! agree on whether an input is valid, and on the value tree when it is.
//!
//! Intentional divergence, neutralised per value:
//! - Integers outside the `i64` range: rs-toml stores integers as `i128`
//!   and accepts them; the `toml` crate rejects the document. Those values
//!   (only) are rewritten to `0` and the rest of the document is compared.
//!
//! Representation differences are normalised in `rs_toml_fuzz::Tree`
//! (NaN sign/payload, omitted seconds/fraction digits in times).
#![no_main]

use libfuzzer_sys::fuzz_target;
use rs_toml::Arena;
use rs_toml_fuzz::{Neutralize, from_rs_toml_table, from_toml_table, neutralize};

fuzz_target!(|data: &[u8]| {
    let Ok(input) = std::str::from_utf8(data) else {
        return;
    };
    let arena = Arena::new();
    let doc = rs_toml::parse(input, &arena);
    let what = Neutralize {
        wide_integers: true,
        datetimes: false,
    };
    // Rewrite only the wide integers, so toml sees the rest of the document.
    let rewritten = doc
        .as_ref()
        .ok()
        .and_then(|doc| neutralize(input, doc.table(), what));
    let ours = doc.map(|doc| {
        let mut tree = from_rs_toml_table(doc.table());
        tree.zero_wide_integers();
        tree
    });
    let compared = rewritten.as_deref().unwrap_or(input);
    let theirs = compared.parse::<toml::Table>().map(|t| from_toml_table(&t));

    match (ours, theirs) {
        (Ok(ours), Ok(theirs)) => assert!(
            ours == theirs,
            "value trees differ\n--- input\n{input:?}\n--- compared\n{compared:?}\n--- rs-toml\n{ours:?}\n--- toml\n{theirs:?}"
        ),
        (Err(_), Err(_)) => {}
        (Ok(_), Err(err)) => {
            panic!(
                "rs-toml accepts, toml rejects\n--- input\n{input:?}\n--- compared\n{compared:?}\n--- toml error\n{err}"
            );
        }
        (Err(err), Ok(_)) => {
            panic!(
                "rs-toml rejects, toml accepts\n--- input\n{input:?}\n--- rs-toml error\n{err:?}"
            );
        }
    }
});
