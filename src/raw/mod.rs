//! The only modules of the crate allowed to use `unsafe`.
//!
//! Everything here exposes a safe API: no caller outside `raw` can cause
//! undefined behaviour through it. The rest of the crate is compiled with
//! `#![deny(unsafe_code)]` and `scripts/check-unsafe.sh` rejects `unsafe`
//! anywhere else.

pub(crate) mod arena;
pub(crate) mod text;
pub(crate) mod vec;
