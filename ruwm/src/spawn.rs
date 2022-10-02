use core::fmt::Debug;

use embedded_graphics::prelude::RgbColor;

use embedded_hal::adc;
use embedded_hal::digital::v2::{InputPin, OutputPin};

use embedded_svc::mqtt::client::asynch::{Client, Connection, Publish};
use embedded_svc::wifi::Wifi as WifiTrait;
use embedded_svc::ws::asynch::server::Acceptor;

use edge_executor::*;
use valve::ValveState;
use wm_stats::WaterMeterStatsState;

use crate::button::{self, PressedLevel};
use crate::channel::Receiver;
use crate::mqtt::MqttCommand;
use crate::pulse_counter::{PulseCounter, PulseWakeup};
use crate::screen::FlushableDrawTarget;
use crate::wm::{self, WaterMeterState};
use crate::{battery, emergency, keepalive, mqtt, screen, web, wm_stats};
use crate::{valve, wifi};

pub fn high_prio_executor<'a, EN, EW, ADC, BP>(
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
    button1_pin: impl InputPin + 'a,
    button2_pin: impl InputPin + 'a,
    button3_pin: impl InputPin + 'a,
) -> Result<(Executor<'a, 16, EN, EW, Local>, heapless::Vec<Task<()>, 16>), SpawnError>
where
    EN: NotifyFactory + RunContextFactory + Default,
    EW: Default,
    ADC: 'a,
    BP: adc::Channel<ADC> + 'a,
{
    let mut executor = Executor::<16, EN, EW, Local>::new();
    let mut tasks = heapless::Vec::<Task<()>, 16>::new();

    executor
        .spawn_local_collect(valve::process(), &mut tasks)?
        .spawn_local_collect(
            valve::spin(valve_power_pin, valve_open_pin, valve_close_pin),
            &mut tasks,
        )?
        .spawn_local_collect(valve::persist(valve_persister), &mut tasks)?
        .spawn_local_collect(wm::process(pulse_counter, pulse_wakeup), &mut tasks)?
        .spawn_local_collect(wm::persist(wm_persister), &mut tasks)?
        .spawn_local_collect(wm_stats::persist(wm_stats_persister), &mut tasks)?
        .spawn_local_collect(
            battery::process(battery_voltage, battery_pin, power_pin),
            &mut tasks,
        )?
        .spawn_local_collect(
            button::button1_process(button1_pin, PressedLevel::Low),
            &mut tasks,
        )?
        .spawn_local_collect(
            button::button2_process(button2_pin, PressedLevel::Low),
            &mut tasks,
        )?
        .spawn_local_collect(
            button::button3_process(button3_pin, PressedLevel::Low),
            &mut tasks,
        )?
        .spawn_local_collect(emergency::process(), &mut tasks)?
        .spawn_local_collect(keepalive::process(), &mut tasks)?;

    Ok((executor, tasks))
}

pub fn mid_prio_executor<'a, EN, EW, D, E>(
    display: D,
    wm_flash: impl FnMut(WaterMeterState) + 'a,
    wifi: impl WifiTrait + 'a,
    wifi_notif: impl Receiver<Data = E> + 'a,
    mqtt_conn: impl Connection<Message = Option<MqttCommand>> + 'a,
) -> Result<(Executor<'a, 8, EN, EW, Local>, heapless::Vec<Task<()>, 8>), SpawnError>
where
    EN: NotifyFactory + RunContextFactory + Default,
    EW: Default,
    D: FlushableDrawTarget + 'a,
    D::Color: RgbColor,
    D::Error: Debug,
    E: 'a,
{
    let mut executor = Executor::<8, EN, EW, Local>::new();
    let mut tasks = heapless::Vec::<Task<()>, 8>::new();

    executor
        .spawn_local_collect(wm_stats::process(), &mut tasks)?
        .spawn_local_collect(screen::process(), &mut tasks)?
        .spawn_local_collect(screen::run_draw(display), &mut tasks)?
        .spawn_local_collect(wifi::process(wifi, wifi_notif), &mut tasks)?
        .spawn_local_collect(mqtt::receive(mqtt_conn), &mut tasks)?
        .spawn_local_collect(wm::flash(wm_flash), &mut tasks)?;

    Ok((executor, tasks))
}

pub fn low_prio_executor<'a, const L: usize, EN, EW>(
    mqtt_topic_prefix: &'a str,
    mqtt_client: impl Client + Publish + 'a,
    ws_acceptor: impl Acceptor + 'a,
) -> Result<(Executor<'a, 4, EN, EW, Local>, heapless::Vec<Task<()>, 4>), SpawnError>
where
    EN: NotifyFactory + RunContextFactory + Default,
    EW: Default,
{
    let mut executor = Executor::<4, EN, EW, Local>::new();
    let mut tasks = heapless::Vec::<Task<()>, 4>::new();

    executor
        .spawn_local_collect(mqtt::send::<L>(mqtt_topic_prefix, mqtt_client), &mut tasks)?
        .spawn_local_collect(web::process::<_, 1>(ws_acceptor), &mut tasks)?;

    Ok((executor, tasks))
}

pub fn run<const C: usize, EN, EW>(
    executor: &mut Executor<C, EN, EW, Local>,
    tasks: heapless::Vec<Task<()>, C>,
) where
    EN: NotifyFactory + RunContextFactory + Default,
    EW: Wait + Default,
{
    use crate::quit;

    executor.with_context(|exec, ctx| {
        exec.run_tasks(ctx, move || !quit::QUIT.is_triggered(), tasks);
    });
}

#[cfg(feature = "std")]
pub fn schedule<'a, const C: usize, EN, EW>(
    stack_size: usize,
    spawner: impl FnOnce() -> Result<(Executor<'a, C, EN, EW, Local>, heapless::Vec<Task<()>, C>), SpawnError>
        + Send
        + 'static,
) -> std::thread::JoinHandle<()>
where
    EN: NotifyFactory + RunContextFactory + Default,
    EW: Wait + Default,
{
    std::thread::Builder::new()
        .stack_size(stack_size)
        .spawn(move || {
            let (mut executor, tasks) = spawner().unwrap();

            // info!(
            //     "Tasks on thread {:?} scheduled, about to run the executor now",
            //     "TODO"
            // );

            run(&mut executor, tasks);
        })
        .unwrap()
}
