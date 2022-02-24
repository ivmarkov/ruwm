#![no_std]
#![feature(generic_associated_types)]
#![feature(type_alias_impl_trait)]

extern crate alloc;

pub mod battery;
pub mod broadcast_binder;
pub mod broadcast_event;
pub mod button;
pub mod emergency;
pub mod event_logger;
pub mod mqtt;
pub mod pipe;
pub mod pulse_counter;
pub mod screen;
pub mod state_snapshot;
pub mod storage;
pub mod valve;
pub mod water_meter;
