use esp_idf_hal::adc::*;
use esp_idf_hal::gpio::*;
use esp_idf_hal::modem::Modem;
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::spi::*;

pub struct SystemPeripherals<P, ADC, V, B1, B2, B3, SPI> {
    pub pulse_counter: P,
    pub valve: ValvePeripherals,
    pub battery: BatteryPeripherals<ADC, V>,
    pub buttons: ButtonsPeripherals<B1, B2, B3>,
    pub display: DisplaySpiPeripherals<SPI>,
    pub modem: Modem,
}

#[cfg(any(esp32, esp32s2, esp32s3))]
impl SystemPeripherals<Gpio33, ADC1, Gpio36, Gpio2, Gpio4, Gpio32, SPI2> {
    pub fn take() -> Self {
        let peripherals = Peripherals::take().unwrap();

        SystemPeripherals {
            pulse_counter: peripherals.pins.gpio33,
            valve: ValvePeripherals {
                power: peripherals.pins.gpio25.into(),
                open: peripherals.pins.gpio26.into(),
                close: peripherals.pins.gpio27.into(),
            },
            battery: BatteryPeripherals {
                power: peripherals.pins.gpio35.into(),
                voltage: peripherals.pins.gpio36,
                adc: peripherals.adc1,
            },
            buttons: ButtonsPeripherals {
                button1: peripherals.pins.gpio2,
                button2: peripherals.pins.gpio4,
                button3: peripherals.pins.gpio32,
            },
            display: DisplaySpiPeripherals {
                control: DisplayControlPeripherals {
                    backlight: Some(peripherals.pins.gpio15.into()),
                    dc: peripherals.pins.gpio18.into(),
                    rst: peripherals.pins.gpio19.into(),
                },
                spi: peripherals.spi2,
                sclk: peripherals.pins.gpio14.into(),
                sdo: peripherals.pins.gpio13.into(),
                cs: Some(peripherals.pins.gpio5.into()),
            },
            modem: peripherals.modem,
        }
    }
}

#[cfg(not(any(esp32, esp32s2, esp32s3)))]
impl SystemPeripherals<Gpio11, ADC1, Gpio0, Gpio5, Gpio6, Gpio7, SPI2> {
    pub fn take() -> Self {
        let peripherals = Peripherals::take().unwrap();

        SystemPeripherals {
            pulse_counter: peripherals.pins.gpio11,
            valve: ValvePeripherals {
                power: peripherals.pins.gpio2.into(),
                open: peripherals.pins.gpio3.into(),
                close: peripherals.pins.gpio4.into(),
            },
            battery: BatteryPeripherals {
                power: peripherals.pins.gpio1.into(),
                voltage: peripherals.pins.gpio0,
                adc: peripherals.adc1,
            },
            buttons: ButtonsPeripherals {
                button1: peripherals.pins.gpio5,
                button2: peripherals.pins.gpio6,
                button3: peripherals.pins.gpio7,
            },
            display: DisplaySpiPeripherals {
                control: DisplayControlPeripherals {
                    backlight: Some(peripherals.pins.gpio8.into()),
                    dc: peripherals.pins.gpio9.into(),
                    rst: peripherals.pins.gpio10.into(),
                },
                spi: peripherals.spi2,
                sclk: peripherals.pins.gpio15.into(),
                sdo: peripherals.pins.gpio16.into(),
                cs: Some(peripherals.pins.gpio14.into()),
            },
            modem: peripherals.modem,
        }
    }
}

pub struct ValvePeripherals {
    pub power: AnyIOPin,
    pub open: AnyIOPin,
    pub close: AnyIOPin,
}

pub struct BatteryPeripherals<ADC, V> {
    pub power: AnyInputPin,
    pub voltage: V,
    pub adc: ADC,
}

pub struct ButtonsPeripherals<B1, B2, B3> {
    pub button1: B1,
    pub button2: B2,
    pub button3: B3,
}

pub struct DisplayControlPeripherals {
    pub backlight: Option<AnyOutputPin>,
    pub dc: AnyOutputPin,
    pub rst: AnyOutputPin,
}

pub struct DisplaySpiPeripherals<SPI> {
    pub control: DisplayControlPeripherals,
    pub spi: SPI,
    pub sclk: AnyOutputPin,
    pub sdo: AnyOutputPin,
    pub cs: Option<AnyOutputPin>,
}
