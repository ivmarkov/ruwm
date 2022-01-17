use core::fmt::{Debug, Display};

#[derive(Clone, Debug)]
pub struct Data {
    pub debounce_edges: u16,
    pub wakeup_edges: u16,
    pub edges_count: u16,
}

impl Default for Data {
    fn default() -> Self {
        Self {
            debounce_edges: 5,
            wakeup_edges: 0,
            edges_count: 0,
        }
    }
}

pub trait PulseCounter {
    type Error: Debug + Display + Send + Sync + 'static;

    fn initialize(&mut self) -> Result<(), Self::Error>;

    fn get_data(&self) -> Result<Data, Self::Error>;

    fn swap_data(&mut self, data: &Data) -> Result<Data, Self::Error>;
}
