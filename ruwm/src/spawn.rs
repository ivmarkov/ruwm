use core::fmt::Debug;

use embedded_hal::adc;
use embedded_hal::digital::v2::{InputPin, OutputPin};

use embedded_svc::mqtt::client::asynch::{Client, Connection, Publish};
use embedded_svc::wifi::Wifi as WifiTrait;
use embedded_svc::ws::asynch::server::Acceptor;

use edge_executor::*;

use channel_bridge::asynch::*;

use valve::ValveState;
use wm_stats::WaterMeterStatsState;

use crate::button::{self, PressedLevel};
use crate::mqtt::MqttCommand;
use crate::pulse_counter::{PulseCounter, PulseWakeup};
use crate::screen::{Color, FlushableDrawTarget};
use crate::web::{self, WebEvent, WebRequest};
use crate::wm::{self, WaterMeterState};
use crate::{battery, emergency, keepalive, mqtt, screen, wm_stats, ws};
use crate::{valve, wifi};

pub fn high_prio<'a, ADC, BP, const C: usize, M>(
    executor: &mut Executor<'a, C, M, Local>,
    tasks: &mut heapless::Vec<Task<()>, C>,
    valve_power_pin: impl OutputPin<Error = impl Debug + 'a> + 'a,
    valve_open_pin: impl OutputPin<Error = impl Debug + 'a> + 'a,
    valve_close_pin: impl OutputPin<Error = impl Debug + 'a> + 'a,
    valve_persister: impl FnMut(Option<ValveState>) + 'a,
    pulse_counter: impl PulseCounter + 'a,
    pulse_wakeup: impl PulseWakeup + 'a,
    wm_persister: impl FnMut(WaterMeterState) + 'a,
    wm_stats_persister: impl FnMut(WaterMeterStatsState) + 'a,
    battery_voltage: impl adc::OneShot<ADC, u16, BP> + 'a,
    battery_pin: BP,
    power_pin: impl InputPin + 'a,
    roller: bool,
    button1_pin: impl InputPin<Error = impl Debug + 'a> + 'a,
    button2_pin: impl InputPin<Error = impl Debug + 'a> + 'a,
    button3_pin: impl InputPin<Error = impl Debug + 'a> + 'a,
) -> Result<(), SpawnError>
where
    M: Monitor + Default,
    ADC: 'a,
    BP: adc::Channel<ADC> + 'a,
{
    executor
        .spawn_local_collect(valve::process(), tasks)?
        .spawn_local_collect(
            valve::spin(valve_power_pin, valve_open_pin, valve_close_pin),
            tasks,
        )?
        .spawn_local_collect(valve::persist(valve_persister), tasks)?
        .spawn_local_collect(wm::process(pulse_counter, pulse_wakeup), tasks)?
        .spawn_local_collect(wm::persist(wm_persister), tasks)?
        .spawn_local_collect(wm_stats::persist(wm_stats_persister), tasks)?
        .spawn_local_collect(
            battery::process(battery_voltage, battery_pin, power_pin),
            tasks,
        )?
        .spawn_local_collect(
            button::button3_process(button3_pin, PressedLevel::Low),
            tasks,
        )?
        .spawn_local_collect(emergency::process(), tasks)?
        .spawn_local_collect(keepalive::process(), tasks)?;

    if roller {
        executor.spawn_local_collect(
            button::button1_button2_roller_process(button1_pin, button2_pin),
            tasks,
        )?;
    } else {
        executor
            .spawn_local_collect(
                button::button1_process(button1_pin, PressedLevel::Low),
                tasks,
            )?
            .spawn_local_collect(
                button::button2_process(button2_pin, PressedLevel::Low),
                tasks,
            )?;
    }

    Ok(())
}

pub fn mid_prio<'a, const C: usize, M, D>(
    executor: &mut Executor<'a, C, M, Local>,
    tasks: &mut heapless::Vec<Task<()>, C>,
    display: D,
    wm_flash: impl FnMut(WaterMeterState) + 'a,
) -> Result<(), SpawnError>
where
    M: Monitor + Default,
    D: FlushableDrawTarget<Color = Color> + 'a,
    D::Error: Debug,
{
    executor
        .spawn_local_collect(wm_stats::process(), tasks)?
        .spawn_local_collect(screen::process(), tasks)?
        .spawn_local_collect(screen::run_draw(display), tasks)?
        .spawn_local_collect(wm::flash(wm_flash), tasks)?;

    Ok(())
}

pub fn wifi<'a, const C: usize, M, D>(
    executor: &mut Executor<'a, C, M, Local>,
    tasks: &mut heapless::Vec<Task<()>, C>,
    wifi: impl WifiTrait + 'a,
    wifi_notif: impl Receiver<Data = D> + 'a,
) -> Result<(), SpawnError>
where
    M: Monitor + Default,
    D: 'a,
{
    executor.spawn_local_collect(wifi::process(wifi, wifi_notif), tasks)?;

    Ok(())
}

pub fn mqtt_send<'a, const L: usize, const C: usize, M>(
    executor: &mut Executor<'a, C, M, Local>,
    tasks: &mut heapless::Vec<Task<()>, C>,
    mqtt_topic_prefix: &'a str,
    mqtt_client: impl Client + Publish + 'a,
) -> Result<(), SpawnError>
where
    M: Monitor + Default,
{
    executor.spawn_local_collect(mqtt::send::<L>(mqtt_topic_prefix, mqtt_client), tasks)?;

    Ok(())
}

pub fn mqtt_receive<'a, const C: usize, M>(
    executor: &mut Executor<'a, C, M, Local>,
    tasks: &mut heapless::Vec<Task<()>, C>,
    mqtt_conn: impl Connection<Message = Option<MqttCommand>> + 'a,
) -> Result<(), SpawnError>
where
    M: Monitor + Default,
{
    executor.spawn_local_collect(mqtt::receive(mqtt_conn), tasks)?;

    Ok(())
}

pub fn web<'a, const C: usize, M, S, R>(
    executor: &mut Executor<'a, C, M, Local>,
    tasks: &mut heapless::Vec<Task<()>, C>,
    sender: S,
    receiver: R,
) -> Result<(), SpawnError>
where
    M: Monitor + Default,
    S: Sender<Data = WebEvent> + 'a,
    R: Receiver<Data = Option<WebRequest>, Error = S::Error> + 'a,
{
    executor.spawn_local_collect(web::process(sender, receiver), tasks)?;

    Ok(())
}

pub fn ws<'a, const C: usize, M>(
    executor: &mut Executor<'a, C, M, Local>,
    tasks: &mut heapless::Vec<Task<()>, C>,
    acceptor: impl Acceptor + 'a,
) -> Result<(), SpawnError>
where
    M: Monitor + Default,
{
    executor.spawn_local_collect(ws::process(acceptor), tasks)?;

    Ok(())
}

pub fn run<const C: usize, M>(
    executor: &mut Executor<C, M, Local>,
    tasks: heapless::Vec<Task<()>, C>,
) where
    M: Monitor + Wait + Default,
{
    executor.run_tasks(move || !crate::quit::QUIT.triggered(), tasks);
}

pub fn start<const C: usize, M, F>(
    executor: &'static mut Executor<C, M, Local>,
    tasks: heapless::Vec<Task<()>, C>,
    finished: F,
) where
    M: Monitor + Start + Default,
    F: FnOnce() + 'static,
{
    executor.start(move || !crate::quit::QUIT.triggered(), {
        let executor = &*executor;

        move || {
            executor.drop_tasks(tasks);

            finished();
        }
    });
}
