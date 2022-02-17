#![no_std]
#![feature(generic_associated_types)]
#![feature(type_alias_impl_trait)]
#![feature(const_generics_defaults)]

extern crate alloc;

pub mod battery;
pub mod button;
pub mod emergency;
pub mod mqtt;
pub mod pipe;
pub mod pulse_counter;
pub mod state_snapshot;
pub mod storage;
pub mod valve;
pub mod water_meter;
