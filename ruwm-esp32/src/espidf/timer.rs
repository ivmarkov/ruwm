use embedded_svc::timer::nonblocking::*;
use embedded_svc::utils::nonblocking::Asyncify;

use esp_idf_svc::timer::*;

use ruwm::error;

pub fn timers() -> error::Result<impl TimerService> {
    Ok(EspTimerService::new()?.into_async())
}
