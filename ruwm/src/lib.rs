#![no_std]
#![feature(generic_associated_types)]
#![feature(type_alias_impl_trait)]
#![feature(explicit_generic_args_with_impl_trait)]

pub mod battery;
pub mod button;
pub mod emergency;
pub mod error;
pub mod event_logger;
pub mod keepalive;
pub mod mqtt;
pub mod pipe;
pub mod pulse_counter;
pub mod screen;
pub mod state_snapshot;
pub mod storage;
pub mod system;
pub mod utils;
pub mod valve;
pub mod water_meter;
pub mod water_meter_stats;
pub mod web;
pub mod web_dto;
pub mod wifi;
