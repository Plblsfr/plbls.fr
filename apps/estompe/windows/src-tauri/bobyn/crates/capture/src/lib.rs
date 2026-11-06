//! Capture WGC, conversion BGRA -> NV12 et horloge, tout sur le GPU.
#![cfg(windows)]
pub mod clock;
pub mod convert;
pub mod gpu;
pub mod wgc;
