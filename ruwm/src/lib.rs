#![no_std]
#![allow(stable_features)]
#![allow(unknown_lints)]
#![feature(async_fn_in_trait)]
#![allow(async_fn_in_trait)]
#![feature(impl_trait_projections)]

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
