#![recursion_limit = "1024"]

use channel_bridge::asynch::Mapper;

use edge_executor::{Executor, Local, SpawnError, WasmMonitor};

use log::info;
use static_cell::StaticCell;

use yew::prelude::*;

use ruwm::{button, spawn};

mod peripherals;
mod services;

//const SLEEP_TIME: Duration = Duration::from_secs(30);
//const MQTT_MAX_TOPIC_LEN: usize = 64;

#[function_component(App)]
pub fn app() -> Html {
    html! {
        <hal_sim::ui::Hal>
            <ruwm_web::App/>
        </hal_sim::ui::Hal>
    }
}

fn main() -> Result<(), SpawnError> {
    wasm_logger::init(wasm_logger::Config::default());
    hal_sim::ui::init_middleware();
    
    yew::start_app::<App>();

    start()?;

    Ok(())
}

static EXECUTOR: StaticCell<Executor<32, WasmMonitor, Local>> = StaticCell::new();

fn start() -> Result<(), SpawnError> {
    info!("Initializing services & peripherals");

    let peripherals = peripherals::SystemPeripherals::take();

    // Valve pins

    let (valve_power_pin, valve_open_pin, valve_close_pin) =
        services::valve_pins(peripherals.valve);

    // Storage

    // #[cfg(feature = "nvs")]
    // let (wm_state, storage) = {
    //     let storage = services::storage(nvs_default_partition.clone())?;

    //     if let Some(wm_state) = storage
    //         .lock(|storage| storage.borrow().get::<WaterMeterState>("wm-state"))
    //         .unwrap()
    //     {
    //         (wm_state, storage)
    //     } else {
    //         log::warn!("No WM edge count found in NVS, assuming new device");

    //         (Default::default(), storage)
    //     }
    // };

    // #[cfg(not(feature = "nvs"))]
    // let wm_state: WaterMeterState = Default::default();

    // unsafe {
    //     services::RTC_MEMORY.wm = wm_state;

    //     ruwm::valve::STATE.set(services::RTC_MEMORY.valve);
    //     ruwm::wm::STATE.set(services::RTC_MEMORY.wm);
    //     ruwm::wm_stats::STATE.set(services::RTC_MEMORY.wm_stats);
    // }

    // Pulse counter

    let (pulse_counter, pulse_wakeup) = services::pulse(peripherals.pulse);

    // TODO
    // Mqtt

    // let (mqtt_topic_prefix, mqtt_client, mqtt_conn) = services::mqtt()?;

    // Executor

    let executor = EXECUTOR.init(Executor::<32, WasmMonitor, _>::new());
    let mut tasks = heapless::Vec::<_, 32>::new();

    // High-prio tasks

    spawn::high_prio(
        executor,
        &mut tasks,
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
        peripherals.battery.adc,
        peripherals.battery.voltage,
        peripherals.battery.power,
        services::button(peripherals.buttons.button1, &button::BUTTON1_PIN_EDGE),
        services::button(peripherals.buttons.button2, &button::BUTTON2_PIN_EDGE),
        services::button(peripherals.buttons.button3, &button::BUTTON3_PIN_EDGE),
    )?;

    // Mid-prio tasks

    let display = peripherals.display;

    spawn::mid_prio(
        executor,
        &mut tasks,
        services::display(display),
        move |_new_state| {
            #[cfg(feature = "nvs")]
            flash_wm_state(storage, _new_state);
        },
    )?;

    // Low-prio tasks

    // TODO
    // MQTT
    // spawn::mqtt_send::<MQTT_MAX_TOPIC_LEN, 4, _, _>(
    //     &mut executor,
    //     &mut tasks,
    //     mqtt_topic_prefix,
    //     mqtt_client,
    // )?;

    // Web
    spawn::web(
        executor,
        &mut tasks,
        ruwm_web::comm::sender(),
        Mapper::new(ruwm_web::comm::receiver(), |data| Some(Some(data))),
    )?;

    // Hal Simulator Web
    executor.spawn_local_collect(
        hal_sim::web::process(
            hal_sim::ui::comm::sender(),
            Mapper::new(hal_sim::ui::comm::receiver(), |data| Some(Some(data))),
            peripherals.shared,
        ),
        &mut tasks,
    )?;

    // Start execution

    log::info!("Starting executor");

    spawn::start(executor, tasks, move || {
        log::info!("Execution finished");
    });

    log::info!("All started");

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
