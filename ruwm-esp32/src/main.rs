#![allow(async_fn_in_trait)]
#![warn(clippy::large_futures)]

use core::pin::pin;

extern crate alloc;

use edge_executor::LocalExecutor;
use edge_http::io::server::DefaultServer;
#[cfg(feature = "nvs")]
use embassy_sync::blocking_mutex::Mutex;
use embassy_time::Duration;

#[cfg(feature = "nvs")]
use embedded_svc::storage::Storage;

use esp_idf_svc::hal::adc::attenuation;
use esp_idf_svc::hal::gpio::*;
use esp_idf_svc::hal::reset::WakeupReason;
use esp_idf_svc::hal::task::block_on;
use esp_idf_svc::hal::task::thread::ThreadSpawnConfiguration;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::nvs::EspDefaultNvsPartition;

use esp_idf_svc::sys::esp;
use esp_idf_svc::timer::EspTaskTimerService;

use ruwm::quit;
use ruwm::spawn;
use ruwm::wm::WaterMeterState;

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

// Make sure that the firmware will contain
// up-to-date build time and package info coming from the binary crate
esp_idf_svc::sys::esp_app_desc!();

fn main() -> Result<(), InitError> {
    esp_idf_svc::hal::task::critical_section::link();
    esp_idf_svc::timer::embassy_time_driver::link();

    let wakeup_reason = WakeupReason::get();

    init()?;

    log::info!("Wakeup reason: {:?}", wakeup_reason);

    run(wakeup_reason)?;

    sleep()?;

    unreachable!()
}

fn init() -> Result<(), InitError> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    esp_idf_svc::io::vfs::initialize_eventfd(5)?;

    Ok(())
}

fn sleep() -> Result<(), InitError> {
    unsafe {
        #[cfg(feature = "ulp")]
        esp!(esp_idf_svc::sys::esp_sleep_enable_ulp_wakeup())?;

        esp!(esp_idf_svc::sys::esp_sleep_enable_timer_wakeup(
            SLEEP_TIME.as_micros()
        ))?;

        log::info!("Going to sleep");

        esp_idf_svc::sys::esp_deep_sleep_start();
    }
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
    let timer_service = EspTaskTimerService::new()?;

    // Storage

    #[cfg(feature = "nvs")]
    let (wm_state, storage) = {
        let storage = services::storage(nvs_default_partition.clone())?;

        if let Some(wm_state) = storage
            .lock(|storage| storage.borrow().get::<WaterMeterState>("wm-state"))
            .unwrap()
        {
            (wm_state, storage)
        } else {
            log::warn!("No WM edge count found in NVS, assuming new device");

            (Default::default(), storage)
        }
    };

    #[cfg(not(feature = "nvs"))]
    let wm_state: WaterMeterState = Default::default();

    unsafe {
        services::RTC_MEMORY.wm = wm_state;

        ruwm::valve::STATE.set(services::RTC_MEMORY.valve);
        ruwm::wm::STATE.set(services::RTC_MEMORY.wm);
        ruwm::wm_stats::STATE.set(services::RTC_MEMORY.wm_stats);
    }

    // Pulse counter

    #[cfg(feature = "ulp")]
    let (pulse_counter, pulse_wakeup) = services::pulse(peripherals.pulse_counter, wakeup_reason)?;

    #[cfg(not(feature = "ulp"))]
    let (pulse_counter, pulse_wakeup) = services::pulse(peripherals.pulse_counter)?;

    // High-prio tasks

    let high_prio_executor = LocalExecutor::<16>::new();

    // TODO: Move off the main thread, as it has a fixed, low priority (1)
    spawn::high_prio(
        &high_prio_executor,
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
        services::adc::<{ attenuation::NONE }, _, _>(
            peripherals.battery.adc,
            peripherals.battery.voltage,
        )?,
        PinDriver::input(peripherals.battery.power)?,
        false,
        services::button(peripherals.buttons.button1)?,
        services::button(peripherals.buttons.button2)?,
        services::button(peripherals.buttons.button3)?,
    );

    // Mid-prio tasks

    log::info!("Starting mid-prio executor");

    ThreadSpawnConfiguration {
        name: Some(b"async-exec-mid\0"),
        ..Default::default()
    }
    .set()
    .unwrap();

    let display_peripherals = peripherals.display;

    let mid_prio_execution = std::thread::Builder::new()
        .stack_size(80000)
        .spawn(move || {
            let executor = LocalExecutor::<8>::new();

            // Wifi

            let mut wifi = services::wifi(
                peripherals.modem,
                sysloop.clone(),
                timer_service,
                Some(nvs_default_partition.clone()),
            )
            .unwrap();

            spawn::wifi(&executor, &mut wifi);

            // Mqtt

            let (mqtt_topic_prefix, mut mqtt_client, mut mqtt_conn) = services::mqtt().unwrap();

            spawn::mqtt_receive(&executor, &mut mqtt_conn);

            spawn::mqtt_send::<MQTT_MAX_TOPIC_LEN, 8>(
                &executor,
                mqtt_topic_prefix,
                &mut mqtt_client,
            );

            // Httpd

            let mut httpd = DefaultServer::new();
            let httpd = pin!(services::run_httpd(&mut httpd));

            executor.spawn(httpd).detach();

            block_on(executor.run(quit::QUIT[1].wait()));
        })
        .unwrap();

    // Low-prio tasks

    log::info!("Starting low-prio executor");

    ThreadSpawnConfiguration {
        name: Some(b"async-exec-low\0"),
        ..Default::default()
    }
    .set()
    .unwrap();

    let low_prio_execution = std::thread::Builder::new()
        .stack_size(60000)
        .spawn(move || {
            let executor = LocalExecutor::<4>::new();

            let mut display = services::display(display_peripherals).unwrap();

            spawn::low_prio(&executor, &mut display, move |_new_state| {
                #[cfg(feature = "nvs")]
                flash_wm_state(storage, _new_state);
            });

            block_on(executor.run(quit::QUIT[2].wait()));
        })
        .unwrap();

    // Start main execution

    log::info!("Starting high-prio executor");

    block_on(high_prio_executor.run(quit::QUIT[0].wait()));

    log::info!("Execution finished, waiting for 2s to workaround a STD/ESP-IDF pthread (?) bug");

    std::thread::sleep(core::time::Duration::from_millis(2000));

    mid_prio_execution.join().unwrap();
    low_prio_execution.join().unwrap();

    log::info!("Finished execution");

    Ok(())
}

#[cfg(feature = "nvs")]
fn flash_wm_state<S>(
    storage: &'static Mutex<
        impl embassy_sync::blocking_mutex::raw::RawMutex,
        core::cell::RefCell<S>,
    >,
    new_state: WaterMeterState,
) where
    S: Storage,
{
    ruwm::log_err!(storage.lock(|storage| {
        let old_state = storage.borrow().get("wm-state")?;
        if old_state != Some(new_state) {
            storage.borrow_mut().set("wm-state", &new_state)?;
        }

        Ok::<_, S::Error>(())
    }));
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
        esp!(esp_idf_svc::sys::esp_sleep_enable_ext1_wakeup(
            mask,
            esp_idf_svc::sys::esp_sleep_ext1_wakeup_mode_t_ESP_EXT1_WAKEUP_ALL_LOW,
        ))?;

        #[cfg(not(any(esp32, esp32s2, esp32s3)))]
        esp!(esp_idf_svc::sys::esp_deep_sleep_enable_gpio_wakeup(
            mask,
            esp_idf_svc::sys::esp_deepsleep_gpio_wake_up_mode_t_ESP_GPIO_WAKEUP_GPIO_LOW,
        ))?;
    }

    Ok(())
}
