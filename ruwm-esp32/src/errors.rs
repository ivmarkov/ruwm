use edge_executor::SpawnError;

use esp_idf_svc::errors::EspIOError;
use esp_idf_sys::EspError;

#[derive(Debug)]
pub enum InitError {
    EspError(EspError),
    SpawnError(SpawnError),
}

impl From<EspError> for InitError {
    fn from(e: EspError) -> Self {
        Self::EspError(e)
    }
}

impl From<EspIOError> for InitError {
    fn from(e: EspIOError) -> Self {
        Self::EspError(e.0)
    }
}

impl From<SpawnError> for InitError {
    fn from(e: SpawnError) -> Self {
        Self::SpawnError(e)
    }
}

//impl std::error::Error for InitError {}
