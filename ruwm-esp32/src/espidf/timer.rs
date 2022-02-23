use embedded_svc::timer::nonblocking::*;
use embedded_svc::utils::nonblocking::Asyncify;

use esp_idf_svc::timer::*;

pub fn timers() -> anyhow::Result<impl TimerService> {
    Ok(EspTimerService::new()?.into_async())
}
