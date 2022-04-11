use embedded_svc::timer::asyncs::*;
use embedded_svc::utils::asyncify::Asyncify;

use esp_idf_svc::timer::*;

use ruwm::error;

pub fn timers() -> error::Result<impl TimerService> {
    Ok(unsafe { EspISRTimerService::new() }?.into_async())
}
