use core::convert::Infallible;
use core::future::Future;
use core::time::Duration;

use embedded_svc::timer::nonblocking::*;
use embedded_svc::utils::nonblocking::Asyncify;

use esp_idf_svc::timer::*;

pub fn oneshot() -> anyhow::Result<impl OnceTimer> {
    Ok(EspTimerService::new()?.into_async().timer()?)
}

pub fn periodic() -> anyhow::Result<impl PeriodicTimer> {
    Ok(EspTimerService::new()?.into_async().timer()?)
}
