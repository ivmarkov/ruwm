#![no_std]
#![allow(async_fn_in_trait)]
#![warn(clippy::large_futures)]

#[cfg(feature = "system")]
pub mod battery;
#[cfg(feature = "system")]
pub mod button;
pub mod dto;
#[cfg(feature = "system")]
pub mod emergency;
#[cfg(feature = "system")]
pub mod error;
#[cfg(feature = "system")]
pub mod keepalive;
#[cfg(feature = "system")]
pub mod mqtt;
#[cfg(feature = "system")]
pub mod pulse_counter;
#[cfg(feature = "system")]
pub mod quit;
#[cfg(feature = "system")]
pub mod screen;
#[cfg(all(feature = "system", feature = "edge-executor"))]
pub mod spawn;
#[cfg(feature = "system")]
pub mod state;
#[cfg(feature = "system")]
pub mod utils;
#[cfg(feature = "system")]
pub mod valve;
#[cfg(feature = "system")]
pub mod web;
#[cfg(feature = "system")]
pub mod wifi;
#[cfg(feature = "system")]
pub mod wm;
#[cfg(feature = "system")]
pub mod wm_stats;
#[cfg(feature = "system")]
pub mod ws;
