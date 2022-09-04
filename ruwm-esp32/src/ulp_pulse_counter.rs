use core::future::Future;
use core::time::Duration;

use embedded_svc::timer::asynch::OnceTimer;

use esp_idf_hal::{
    gpio::RTCPin,
    interrupt::CriticalSection,
    peripheral::{Peripheral, PeripheralRef},
    ulp,
};
use esp_idf_sys::EspError;

use ruwm::pulse_counter;

#[cfg(all(feature = "ulp", not(any(esp32, esp32s2))))]
compile_error!("Feature `ulp` can be enabled only on esp32 and esp32s2");

mod ulp_code_vars {
    include!(env!("ULP_FSM_RS"));
}

pub struct UlpPulseCounter<'d, T, P> {
    driver: ulp::UlpDriver<'d>,
    timer: T,
    pin: PeripheralRef<'d, P>,
}

impl<'d, T, P> UlpPulseCounter<'d, T, P>
where
    P: RTCPin,
{
    const ULP_CODE: &'static [u8] = include_bytes!(env!("ULP_FSM_BIN"));

    pub fn new(
        driver: ulp::UlpDriver<'d>,
        timer: T,
        pin: impl Peripheral<P = P> + 'd,
        cold_boot: bool,
    ) -> Result<Self, EspError> {
        esp_idf_hal::into_ref!(pin);

        let mut this = Self { driver, timer, pin };

        if cold_boot {
            this.initialize()?;
            this.start()?;
        }

        Ok(this)
    }

    pub fn split(
        &mut self,
    ) -> (
        &mut (impl pulse_counter::PulseCounter + 'd),
        &mut (impl pulse_counter::PulseWakeup + 'd),
    )
    where
        T: OnceTimer + 'd,
    {
        let ptr: *mut Self = self;

        // This is safe because the access to the Ulp driver is protected with critical sections
        unsafe { (ptr.as_mut().unwrap(), ptr.as_mut().unwrap()) }
    }

    fn initialize(&mut self) -> Result<(), EspError> {
        unsafe {
            self.driver.load(Self::ULP_CODE)?;

            self.driver.write_word(ulp_code_vars::edge_count, 0)?;

            self.driver
                .write_word(ulp_code_vars::edge_count_to_wake_up, 0)?;

            self.driver
                .write_word(ulp_code_vars::debounce_max_count, 5)?;

            self.driver
                .write_word(ulp_code_vars::io_number, self.pin.rtc_pin() as _)?;
        }

        Ok(())
    }

    fn start(&mut self) -> Result<(), EspError> {
        unsafe { self.driver.start(ulp_code_vars::entry) }
    }

    // fn stop(&mut self) -> Result<(), EspError> {
    //     self.driver.stop()
    // }
}

impl<'d, T, P> pulse_counter::PulseCounter for UlpPulseCounter<'d, T, P>
where
    T: OnceTimer,
    P: 'd,
{
    type Error = EspError;

    type TakePulsesFuture<'a> = impl Future<Output = Result<u64, Self::Error>> where Self: 'a;

    fn take_pulses(&mut self) -> Self::TakePulsesFuture<'_> {
        async move {
            self.timer
                .after(Duration::from_secs(2) /*TODO*/)
                .unwrap()
                .await;

            let edges_count = {
                let _cs = CriticalSection::new();

                unsafe {
                    let edges_count = self.driver.read_word(ulp_code_vars::edge_count)?.value();

                    self.driver.write_word(ulp_code_vars::edge_count, 0)?;

                    edges_count
                }
            };

            Ok(edges_count as _)
        }
    }
}

impl<'d, T, P> pulse_counter::PulseWakeup for UlpPulseCounter<'d, T, P>
where
    P: RTCPin,
{
    type Error = EspError;

    fn set_enabled(&mut self, enabled: bool) -> Result<(), Self::Error> {
        let _cs = CriticalSection::new();

        let wakeup_edges = unsafe {
            self.driver
                .read_word(ulp_code_vars::edge_count_to_wake_up)?
                .value()
        };

        if enabled != (wakeup_edges > 0) {
            unsafe {
                self.driver.write_word(
                    ulp_code_vars::edge_count_to_wake_up,
                    if enabled { 1 } else { 0 },
                )?;
            }
        }

        Ok(())
    }
}
