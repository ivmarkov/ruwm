use core::fmt::Debug;

use crate::error;

#[derive(Clone, Debug)]
pub struct Data {
    pub debounce_edges: u16,
    pub wakeup_edges: u16,
    pub edges_count: u16,
    pub pin_no: u16,
}

impl Default for Data {
    fn default() -> Self {
        Self {
            debounce_edges: 5,
            wakeup_edges: 0,
            edges_count: 0,
            pin_no: 16,
        }
    }
}

pub trait PulseCounter {
    type Error: error::FullError;

    fn initialize(self) -> Result<Self, Self::Error>
    where
        Self: Sized;

    fn start(&mut self) -> Result<(), Self::Error>;
    fn stop(&mut self) -> Result<(), Self::Error>;

    fn get_data(&self) -> Result<Data, Self::Error>;
    fn swap_data(&mut self, data: &Data) -> Result<Data, Self::Error>;
}
