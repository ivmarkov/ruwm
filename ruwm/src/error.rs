use core::fmt::{Debug, Display};

pub type Result<T> = anyhow::Result<T>;
pub type Error = anyhow::Error;

pub trait HalError: Debug {}

impl<E> HalError for E where E: Debug {}

pub fn svc(e: impl FullError) -> Error {
    full(e)
}

pub fn hal(e: impl HalError) -> Error {
    debug(e)
}

#[cfg(feature = "std")]
pub trait FullError: std::error::Error + Send + Sync + 'static {}

#[cfg(not(feature = "std"))]
pub trait FullError: Debug + Display + Send + Sync + 'static {}

#[cfg(not(feature = "std"))]
impl<E> FullError for E where E: Debug + Display + Send + Sync + 'static {}

#[cfg(feature = "std")]
impl<E> FullError for E where E: std::error::Error + Send + Sync + 'static {}

pub fn full(e: impl FullError) -> Error {
    anyhow::anyhow!(e)
}

pub fn display(e: impl Display) -> Error {
    anyhow::anyhow!("Error: {}", e)
}

pub fn debug(e: impl Debug) -> Error {
    anyhow::anyhow!("Error: {:?}", e)
}
