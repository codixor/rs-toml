//! Differential target: rs-toml and the `toml` crate (both TOML 1.1) must
//! agree on whether an input is valid, and on the value tree when it is.
//!
//! Intentional divergence (inputs ignored, not reported):
//! - Integers outside the `i64` range: rs-toml stores integers as `i128`
//!   and accepts them; the `toml` crate rejects them.
//!
//! Representation differences are normalised in `rs_toml_fuzz::Tree`
//! (NaN sign/payload, omitted seconds/fraction digits in times).
#![no_main]

use libfuzzer_sys::fuzz_target;
use rs_toml::Arena;
use rs_toml_fuzz::{from_rs_toml_table, from_toml_table};

fuzz_target!(|data: &[u8]| {
    let Ok(input) = std::str::from_utf8(data) else {
        return;
    };
    let arena = Arena::new();
    let ours = rs_toml::parse(input, &arena).map(|doc| from_rs_toml_table(doc.table()));
    let theirs = input.parse::<toml::Table>().map(|t| from_toml_table(&t));

    match (ours, theirs) {
        (Ok(ours), Ok(theirs)) => assert!(
            ours == theirs,
            "value trees differ\n--- input\n{input:?}\n--- rs-toml\n{ours:?}\n--- toml\n{theirs:?}"
        ),
        (Err(_), Err(_)) => {}
        (Ok(ours), Err(err)) => {
            if ours.has_wide_integer() {
                return;
            }
            panic!("rs-toml accepts, toml rejects\n--- input\n{input:?}\n--- toml error\n{err}");
        }
        (Err(err), Ok(_)) => {
            panic!(
                "rs-toml rejects, toml accepts\n--- input\n{input:?}\n--- rs-toml error\n{err:?}"
            );
        }
    }
});
