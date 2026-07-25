//! Reusable Nintendo DS ARM9 loader and progressively translated CPU runtime.
//!
//! ARM9 code is decoded to a small IR first. This keeps DS memory and BIOS
//! behavior separate from the eventual Vita ARMv7 code emitter.

pub mod arm;
pub mod nds;
