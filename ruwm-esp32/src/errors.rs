use esp_idf_svc::io::EspIOError;
use esp_idf_svc::sys::EspError;

#[derive(Debug)]
pub enum InitError {
    EspError(EspError),
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

//impl std::error::Error for InitError {}
