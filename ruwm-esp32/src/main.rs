#![feature(cfg_version)]
#![cfg_attr(not(version("1.65")), feature(generic_associated_types))]
#![feature(type_alias_impl_trait)]

extern crate alloc;

#[cfg(feature = "nvs")]
use embassy_sync::blocking_mutex::Mutex;
use embassy_time::Duration;

use embedded_svc::storage::Storage;

use esp_idf_hal::adc::*;
use esp_idf_hal::executor::{CurrentTaskWait, TaskHandle};
use esp_idf_hal::gpio::*;
use esp_idf_hal::reset::WakeupReason;
use esp_idf_hal::task::thread::ThreadSpawnConfiguration;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::nvs::EspDefaultNvsPartition;

use esp_idf_sys::esp;

use ruwm::button;
use ruwm::spawn;

use crate::errors::*;
use crate::peripherals::{ButtonsPeripherals, PulseCounterPeripherals};

mod errors;
mod peripherals;
mod services;
#[cfg(feature = "ulp")]
mod ulp_pulse_counter;

#[cfg(all(feature = "ulp", not(any(esp32, esp32s2, esp32s3))))]
compile_error!("Feature `ulp` is supported only on esp32, esp32s2 and esp32s3");

const SLEEP_TIME: Duration = Duration::from_secs(30);
const MQTT_MAX_TOPIC_LEN: usize = 64;

fn main() -> Result<(), InitError> {
    esp_idf_hal::cs::critical_section::link();
    //esp_idf_hal::timer::embassy_time::queue::link();
    esp_idf_svc::timer::embassy_time::driver::link();
    esp_idf_svc::timer::embassy_time::queue::link();

    let wakeup_reason = WakeupReason::get();

    init()?;

    log::info!("Wakeup reason: {:?}", wakeup_reason);

    run(wakeup_reason)?;

    sleep()?;

    unreachable!()
}

fn init() -> Result<(), InitError> {
    esp_idf_sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    esp!(unsafe {
        #[allow(clippy::needless_update)]
        esp_idf_sys::esp_vfs_eventfd_register(&esp_idf_sys::esp_vfs_eventfd_config_t {
            max_fds: 5,
            ..Default::default()
        })
    })?;

    Ok(())
}

fn sleep() -> Result<(), InitError> {
    unsafe {
        #[cfg(feature = "ulp")]
        esp!(esp_idf_sys::esp_sleep_enable_ulp_wakeup())?;

        esp!(esp_idf_sys::esp_sleep_enable_timer_wakeup(
            SLEEP_TIME.as_micros() as u64
        ))?;

        log::info!("Going to sleep");

        esp_idf_sys::esp_deep_sleep_start();
    }

    Ok(())
}

fn run(wakeup_reason: WakeupReason) -> Result<(), InitError> {
    let peripherals = peripherals::SystemPeripherals::take();

    // Valve pins

    let (valve_power_pin, valve_open_pin, valve_close_pin) =
        services::valve_pins(peripherals.valve, wakeup_reason)?;

    // Deep sleep wakeup init

    mark_wakeup_pins(&peripherals.pulse_counter, &peripherals.buttons)?;

    // ESP-IDF basics

    let nvs_default_partition = EspDefaultNvsPartition::take()?;
    let sysloop = EspSystemEventLoop::take()?;

    // Storage

    let storage = services::storage(nvs_default_partition.clone())?;

    unsafe {
        services::RTC_MEMORY.wm = storage
            .lock(|storage| storage.borrow().get("wm-state"))
            .unwrap()
            .unwrap_or_default();

        ruwm::valve::STATE.set(services::RTC_MEMORY.valve);
        ruwm::wm::STATE.set(services::RTC_MEMORY.wm);
        ruwm::wm_stats::STATE.set(services::RTC_MEMORY.wm_stats);
    }

    // Pulse counter

    #[cfg(feature = "ulp")]
    let (pulse_counter, pulse_wakeup) = services::pulse(peripherals.pulse_counter, wakeup_reason)?;

    #[cfg(not(feature = "ulp"))]
    let (pulse_counter, pulse_wakeup) = services::pulse(peripherals.pulse_counter)?;

    // Wifi

    let (wifi, wifi_notif) = services::wifi(
        peripherals.modem,
        sysloop.clone(),
        Some(nvs_default_partition.clone()),
    )?;

    // Httpd

    let (_httpd, ws_acceptor) = services::httpd()?;

    // Mqtt

    let (mqtt_topic_prefix, mqtt_client, mqtt_conn) = services::mqtt()?;

    // Storage

    #[cfg(feature = "nvs")]
    let storage: &'static Mutex<_, _> = services::storage(nvs_default_partition)?;

    // High-prio executor

    let (mut high_prio_executor, high_prio_tasks) =
        spawn::high_prio_executor::<TaskHandle, CurrentTaskWait, _, _>(
            valve_power_pin,
            valve_open_pin,
            valve_close_pin,
            |state| unsafe {
                services::RTC_MEMORY.valve = state;
            },
            pulse_counter,
            pulse_wakeup,
            |state| unsafe {
                services::RTC_MEMORY.wm = state;
            },
            |state| unsafe {
                services::RTC_MEMORY.wm_stats = state;
            },
            AdcDriver::new(peripherals.battery.adc, &AdcConfig::new().calibration(true))?,
            AdcChannelDriver::<_, Atten0dB<_>>::new(peripherals.battery.voltage)?,
            PinDriver::input(peripherals.battery.power)?,
            services::button(peripherals.buttons.button1, &button::BUTTON1_PIN_EDGE)?,
            services::button(peripherals.buttons.button2, &button::BUTTON2_PIN_EDGE)?,
            services::button(peripherals.buttons.button3, &button::BUTTON3_PIN_EDGE)?,
        )?;

    // Mid-prio executor

    log::info!("Starting mid-prio executor");

    ThreadSpawnConfiguration {
        name: Some(b"async-exec-mid\0"),
        ..Default::default()
    }
    .set()
    .unwrap();

    let display_peripherals = peripherals.display;

    let mid_prio_execution = spawn::schedule::<8, TaskHandle, CurrentTaskWait>(50000, move || {
        spawn::mid_prio_executor(
            services::display(display_peripherals).unwrap(),
            move |_state| {
                #[cfg(feature = "nvs")]
                ruwm::log_err!(
                    storage.lock(|storage| storage.borrow_mut().set("wm-state", &_state))
                );
            },
            wifi,
            wifi_notif,
            mqtt_conn,
        )
    });

    // Low-prio executor

    log::info!("Starting low-prio executor");

    ThreadSpawnConfiguration {
        name: Some(b"async-exec-low\0"),
        ..Default::default()
    }
    .set()
    .unwrap();

    let low_prio_execution = spawn::schedule::<4, TaskHandle, CurrentTaskWait>(50000, move || {
        spawn::low_prio_executor::<MQTT_MAX_TOPIC_LEN, _, _>(
            mqtt_topic_prefix,
            mqtt_client,
            ws_acceptor,
        )
    });

    // Start main execution

    log::info!("Starting high-prio executor");

    spawn::run(&mut high_prio_executor, high_prio_tasks);

    log::info!("Execution finished, waiting for 2s to workaround a STD/ESP-IDF pthread (?) bug");

    std::thread::sleep(core::time::Duration::from_millis(2000));

    mid_prio_execution.join().unwrap();
    low_prio_execution.join().unwrap();

    log::info!("Finished execution");

    Ok(())
}

fn mark_wakeup_pins(
    pulse_counter_peripherals: &PulseCounterPeripherals<impl RTCPin + InputPin>,
    buttons_peripherals: &ButtonsPeripherals<
        impl RTCPin + InputPin,
        impl RTCPin + InputPin,
        impl RTCPin + InputPin,
    >,
) -> Result<(), InitError> {
    unsafe {
        let mut mask = (1 << buttons_peripherals.button1.pin())
            | (1 << buttons_peripherals.button2.pin())
            | (1 << buttons_peripherals.button3.pin());

        #[cfg(not(feature = "ulp"))]
        {
            mask |= 1 << pulse_counter_peripherals.pulse.pin();
        }

        #[cfg(any(esp32, esp32s2, esp32s3))]
        esp!(esp_idf_sys::esp_sleep_enable_ext1_wakeup(
            mask,
            esp_idf_sys::esp_sleep_ext1_wakeup_mode_t_ESP_EXT1_WAKEUP_ALL_LOW,
        ))?;

        #[cfg(not(any(esp32, esp32s2, esp32s3)))]
        esp!(esp_idf_sys::esp_deep_sleep_enable_gpio_wakeup(
            mask,
            esp_idf_sys::esp_deepsleep_gpio_wake_up_mode_t_ESP_GPIO_WAKEUP_GPIO_LOW,
        ))?;
    }

    Ok(())
}
