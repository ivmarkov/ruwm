use core::fmt::Debug;

use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal_async::digital::Wait;

use embedded_svc::mqtt::client::asynch::{Client, Connection, Publish};
use embedded_svc::wifi::asynch::Wifi;
use embedded_svc::ws::asynch::server::Acceptor;

use gfx_xtra::draw_target::Flushable;

use edge_executor::*;

use channel_bridge::asynch::*;

use valve::ValveState;
use wm_stats::WaterMeterStatsState;

use crate::battery::Adc;
use crate::button::{self, PressedLevel};
use crate::pulse_counter::{PulseCounter, PulseWakeup};
use crate::screen::Color;
use crate::web::{self, WebEvent, WebRequest};
use crate::wm::{self, WaterMeterState};
use crate::{battery, emergency, keepalive, mqtt, screen, wm_stats, ws};
use crate::{valve, wifi};

pub fn high_prio<'a, const C: usize>(
    executor: &LocalExecutor<'a, C>,
    valve_power_pin: impl OutputPin<Error = impl Debug + 'a> + 'a,
    valve_open_pin: impl OutputPin<Error = impl Debug + 'a> + 'a,
    valve_close_pin: impl OutputPin<Error = impl Debug + 'a> + 'a,
    valve_persister: impl FnMut(Option<ValveState>) + 'a,
    pulse_counter: impl PulseCounter + 'a,
    pulse_wakeup: impl PulseWakeup + 'a,
    wm_persister: impl FnMut(WaterMeterState) + 'a,
    wm_stats_persister: impl FnMut(WaterMeterStatsState) + 'a,
    battery_voltage: impl Adc + 'a,
    power_pin: impl InputPin + 'a,
    _roller: bool,
    button1_pin: impl InputPin<Error = impl Debug + 'a> + Wait + 'a,
    button2_pin: impl InputPin<Error = impl Debug + 'a> + Wait + 'a,
    button3_pin: impl InputPin<Error = impl Debug + 'a> + Wait + 'a,
) {
    executor.spawn(valve::process()).detach();

    executor
        .spawn(valve::spin(
            valve_power_pin,
            valve_open_pin,
            valve_close_pin,
        ))
        .detach();

    executor.spawn(valve::persist(valve_persister)).detach();

    executor
        .spawn(wm::process(pulse_counter, pulse_wakeup))
        .detach();

    executor.spawn(wm::persist(wm_persister)).detach();

    executor
        .spawn(wm_stats::persist(wm_stats_persister))
        .detach();

    executor
        .spawn(battery::process(battery_voltage, power_pin))
        .detach();

    executor
        .spawn(button::button3_process(button3_pin, PressedLevel::Low))
        .detach();

    executor.spawn(emergency::process()).detach();

    executor.spawn(keepalive::process()).detach();

    // if roller {
    //     executor
    //         .spawn(button::button1_button2_roller_process(button1_pin, button2_pin))
    //         .detach();
    // } else {
    executor
        .spawn(button::button1_process(button1_pin, PressedLevel::Low))
        .detach();

    executor
        .spawn(button::button2_process(button2_pin, PressedLevel::Low))
        .detach();
    // }
}

pub fn mid_prio<'a, const C: usize, D>(
    executor: &LocalExecutor<'a, C>,
    display: D,
    wm_flash: impl FnMut(WaterMeterState) + 'a,
) where
    D: Flushable<Color = Color> + 'a,
    D::Error: Debug,
{
    executor.spawn(wm_stats::process()).detach();

    executor.spawn(screen::process()).detach();

    executor.spawn(screen::run_draw(display)).detach();

    executor.spawn(wm::flash(wm_flash)).detach();
}

pub fn wifi<'a, const C: usize>(executor: &LocalExecutor<'a, C>, wifi: impl Wifi + 'a) {
    executor.spawn(wifi::process(wifi)).detach();
}

pub fn mqtt_send<'a, const L: usize, const C: usize>(
    executor: &LocalExecutor<'a, C>,
    mqtt_topic_prefix: &'a str,
    mqtt_client: impl Client + Publish + 'a,
) {
    executor
        .spawn(mqtt::send::<L>(mqtt_topic_prefix, mqtt_client))
        .detach();
}

pub fn mqtt_receive<const C: usize>(
    executor: &LocalExecutor<'_, C>,
    mqtt_conn: impl Connection + 'static,
) {
    executor.spawn(mqtt::receive(mqtt_conn)).detach();
}

pub fn web<'a, const C: usize, S, R>(executor: &LocalExecutor<'a, C>, sender: S, receiver: R)
where
    S: Sender<Data = WebEvent> + 'a,
    R: Receiver<Data = Option<WebRequest>, Error = S::Error> + 'a,
{
    executor.spawn(web::process(sender, receiver)).detach();
}

pub fn ws<'a, const C: usize>(executor: &LocalExecutor<'a, C>, acceptor: impl Acceptor + 'a) {
    executor.spawn(ws::process(acceptor)).detach();
}
