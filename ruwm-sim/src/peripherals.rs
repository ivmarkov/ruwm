use embedded_graphics_core::pixelcolor::Rgb888;
use embedded_graphics_core::prelude::IntoStorage;

use hal_sim::adc::*;
use hal_sim::display::*;
use hal_sim::gpio::*;
use hal_sim::peripherals::*;
use hal_sim::web;

pub struct SystemPeripherals {
    pub shared: SharedPeripherals,
    pub pulse: Pin<Input>,
    pub valve: ValvePeripherals,
    pub battery: BatteryPeripherals,
    pub buttons: ButtonsPeripherals,
    pub display: Display<Rgb888>,
}

impl SystemPeripherals {
    pub fn take() -> Self {
        let mut peripherals = Peripherals::take(web::peripherals_callback).unwrap();

        SystemPeripherals {
            shared: peripherals.shared(),
            pulse: peripherals.pins.input("Pulse", "Pulse Counter", false),
            valve: ValvePeripherals {
                power: peripherals.pins.output("Power", "Valve", false),
                open: peripherals.pins.output("Open", "Valve", false),
                close: peripherals.pins.output("Close", "Valve", false),
            },
            battery: BatteryPeripherals {
                power: peripherals.pins.input("Power", "Battery", false),
                voltage: peripherals.pins.adc("Voltage", "Battery", 3300),
                adc: peripherals.adc0,
            },
            buttons: ButtonsPeripherals {
                button1: peripherals.pins.input("Button 1", "Buttons", false),
                button2: peripherals.pins.input("Button 2", "Buttons", false),
                button3: peripherals.pins.input("Button 3", "Buttons", false),
            },
            display: peripherals
                .displays
                .display("Display".into(), 320, 240, |color: Rgb888| {
                    color.into_storage()
                }),
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
