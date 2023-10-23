use embedded_graphics_core::pixelcolor::Rgb888;
use embedded_graphics_core::prelude::{IntoStorage, Size};

use hal_sim::adc::*;
use hal_sim::display::*;
use hal_sim::gpio::*;
use hal_sim::io;
use hal_sim::peripherals::*;

use ruwm::battery::BatteryState;

//pub const DISPLAY_SIZE: Size = Size::new(320, 240);
pub const DISPLAY_SIZE: Size = Size::new(128, 128);

pub struct SystemPeripherals {
    pub pulse: Pin<Input>,
    pub valve: ValvePeripherals,
    pub battery: BatteryPeripherals,
    pub buttons: ButtonsPeripherals,
    pub display: Display<Rgb888>,
}

impl SystemPeripherals {
    pub fn take() -> Self {
        let mut peripherals = Peripherals::take(io::peripherals_callback).unwrap();

        SystemPeripherals {
            pulse: peripherals
                .pins
                .input_click("Pulse", "Pulse Counter", false),
            valve: ValvePeripherals {
                power: peripherals.pins.output("Power", "Valve", false),
                open: peripherals.pins.output("Open", "Valve", false),
                close: peripherals.pins.output("Close", "Valve", false),
            },
            battery: BatteryPeripherals {
                power: peripherals.pins.input("Charging", "Battery", false),
                voltage: peripherals.pins.adc_range(
                    "Voltage",
                    "Battery",
                    BatteryState::LOW_VOLTAGE,
                    BatteryState::MAX_VOLTAGE,
                    (BatteryState::LOW_VOLTAGE + BatteryState::MAX_VOLTAGE) / 2,
                ),
                adc: peripherals.adc0,
            },
            buttons: ButtonsPeripherals {
                button1: peripherals.pins.input_click("Prev", "Display", false),
                button2: peripherals.pins.input_click("Next", "Display", false),
                button3: peripherals.pins.input_click("Action", "Display", false),
            },
            display: peripherals.displays.display(
                "Display",
                DISPLAY_SIZE.width as _,
                DISPLAY_SIZE.height as _,
                |color: Rgb888| color.into_storage(),
            ),
        }
    }
}

pub struct ValvePeripherals {
    pub power: Pin<Output>,
    pub open: Pin<Output>,
    pub close: Pin<Output>,
}

pub struct BatteryPeripherals {
    pub power: Pin<Input>,
    pub voltage: Pin<Adc<0>>,
    pub adc: Adc<0>,
}

pub struct ButtonsPeripherals {
    pub button1: Pin<Input>,
    pub button2: Pin<Input>,
    pub button3: Pin<Input>,
}
