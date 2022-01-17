use esp_idf_hal::ulp;
use esp_idf_sys::EspError;

use ruwm::pulse_counter;

mod ulp_code_vars {
    include!(env!("ULP_FSM_RS"));
}

pub struct PulseCounter(ulp::ULP);

impl PulseCounter {
    const ULP_CODE: &'static [u8] = include_bytes!(env!("ULP_FSM_BIN"));

    pub fn new(ulp: ulp::ULP) -> Self {
        Self(ulp)
    }

    pub fn ulp(&self) -> &ulp::ULP {
        &self.0
    }

    pub fn ulp_mut(&mut self) -> &mut ulp::ULP {
        &mut self.0
    }
}

impl pulse_counter::PulseCounter for PulseCounter {
    type Error = EspError;

    fn initialize(&mut self) -> Result<(), Self::Error> {
        unsafe {
            self.ulp_mut().load(Self::ULP_CODE)?;
        }
        self.swap_data(&Default::default())?;

        Ok(())
    }

    fn get_data(&self) -> Result<pulse_counter::Data, Self::Error> {
        unsafe {
            Ok(pulse_counter::Data {
                edges_count: self
                    .ulp()
                    .read_word(ulp_code_vars::edge_count as *const _)?
                    .value(),
                wakeup_edges: self
                    .ulp()
                    .read_word(ulp_code_vars::edge_count_to_wake_up as *const _)?
                    .value(),
                debounce_edges: self
                    .ulp()
                    .read_word(ulp_code_vars::debounce_max_count as *const _)?
                    .value(),
            })
        }
    }

    fn swap_data(
        &mut self,
        data: &pulse_counter::Data,
    ) -> Result<pulse_counter::Data, Self::Error> {
        let mut out_data: pulse_counter::Data = Default::default();

        unsafe {
            out_data.edges_count = self
                .ulp()
                .read_word(ulp_code_vars::edge_count as *const _)?
                .value();
            self.ulp_mut()
                .write_word(ulp_code_vars::edge_count as _, data.edges_count)?;

            out_data.wakeup_edges = self
                .ulp()
                .read_word(ulp_code_vars::edge_count_to_wake_up as *const _)?
                .value();
            self.ulp_mut()
                .write_word(ulp_code_vars::edge_count_to_wake_up as _, data.wakeup_edges)?;

            out_data.debounce_edges = self
                .ulp()
                .read_word(ulp_code_vars::debounce_max_count as *const _)?
                .value();
            self.ulp_mut()
                .write_word(ulp_code_vars::debounce_max_count as _, data.debounce_edges)?;
        }

        Ok(out_data)
    }
}
