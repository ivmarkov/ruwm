use core::fmt::Debug;

use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValveState {
    Open,
    Closed,
    Opening(u8),
    Closing(u8),
}

impl ValveState {
    pub fn open_percentage(&self) -> u8 {
        match self {
            Self::Open => 100,
            Self::Closed => 0,
            Self::Opening(percentage) => *percentage,
            Self::Closing(percentage) => 100 - *percentage,
        }
    }

    pub fn simplify(&self) -> Self {
        match self {
            Self::Opening(_) => Self::Opening(0),
            Self::Closing(_) => Self::Closing(0),
            _ => *self,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub enum ValveCommand {
    Open,
    Close,
}
