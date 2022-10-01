#![cfg_attr(not(feature = "std"), no_std)]
#![feature(cfg_version)]
#![cfg_attr(not(version("1.65")), feature(generic_associated_types))]
#![cfg_attr(not(version("1.64")), feature(future_poll_fn))]
#![feature(type_alias_impl_trait)]

#[cfg(feature = "system")]
pub mod battery;
#[cfg(feature = "system")]
pub mod button;
#[cfg(feature = "system")]
pub mod channel;
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
pub mod notification;
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
