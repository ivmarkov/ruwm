use esp_idf_hal::ulp;
use esp_idf_sys::EspError;

mod ulp_code_vars {
    include!(env!("ULP_FSM_RS"));
}

#[derive(Clone, Debug)]
pub struct Data {
    pub debounce_edges: u16,
    pub wakeup_edges: u16,
    pub edges_count: u16,
}

impl Default for Data {
    fn default() -> Self {
        Self {
            debounce_edges: 5,
            wakeup_edges: 0,
            edges_count: 0,
        }
    }
}

pub struct PulseCounter(ulp::ULP);

impl PulseCounter {
    const ULP_CODE: &'static [u8] = include_bytes!(env!("ULP_FSM_BIN"));

    pub fn new(ulp: ulp::ULP) -> Self {
        Self(ulp)
    }

    pub fn initialize(&mut self) -> Result<(), EspError> {
        unsafe {
            self.ulp_mut().load(Self::ULP_CODE)?;
        }
        self.swap_data(&Default::default())?;

        Ok(())
    }

    pub fn ulp(&self) -> &ulp::ULP {
        &self.0
    }

    pub fn ulp_mut(&mut self) -> &mut ulp::ULP {
        &mut self.0
    }

    pub fn get_data(&self) -> Result<Data, EspError> {
        unsafe {
            Ok(Data {
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

    pub fn swap_data(&mut self, data: &Data) -> Result<Data, EspError> {
        let mut out_data: Data = Default::default();

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
