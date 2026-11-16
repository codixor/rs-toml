//! Deserialize the `vault` service's import configuration
//! (`rs_toml_fuzz::vault::import::Config`) with rs-toml and with the `toml`
//! crate: never panic, agree on accept/reject and on the value. See
//! `rs_toml_fuzz::check_serde` for the intentional divergences.
#![no_main]

use libfuzzer_sys::fuzz_target;
use rs_toml_fuzz::vault::import::Config;

fuzz_target!(|data: &[u8]| {
    let Ok(input) = std::str::from_utf8(data) else {
        return;
    };
    rs_toml_fuzz::check_serde::<Config>(input);
});
