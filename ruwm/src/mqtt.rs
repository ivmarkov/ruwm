use core::str::{self, FromStr};
use core::time::Duration;

use log::{error, info};

use serde::{Deserialize, Serialize};

use heapless::String;

use embassy_futures::select::{select4, Either4};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;

use embedded_svc::mqtt::client::asynch::{Client, Connection, Event, Message, Publish, QoS};
use embedded_svc::mqtt::client::Details;

use crate::battery::{self, BatteryState};
use crate::notification::Notification;
use crate::valve::{ValveCommand, ValveState};
use crate::wm::{WaterMeterCommand, WaterMeterState};
use crate::{error, valve, wm};

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct MqttConfiguration {
    protocol_311: bool,
    url: heapless::String<128>,
    client_id: heapless::String<64>,
    username: heapless::String<64>,
    password: heapless::String<64>,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub enum MqttCommand {
    KeepAlive(Duration),
    Valve(bool),
    FlowWatch(bool),
    SystemUpdate,
}

pub type MqttClientNotification = Result<Event<Option<MqttCommand>>, ()>;

// TODO: Web: connected info at least
static PUBLISH_NOTIFY: &[&Notification] =
    &[&crate::keepalive::NOTIF, &crate::screen::MQTT_STATE_NOTIF];
static RECEIVE_NOTIFY: &[&Notification] =
    &[&crate::keepalive::NOTIF, &crate::screen::MQTT_STATE_NOTIF];

pub(crate) static VALVE_STATE_NOTIF: Notification = Notification::new();
pub(crate) static WM_STATE_NOTIF: Notification = Notification::new();
pub(crate) static BATTERY_STATE_NOTIF: Notification = Notification::new();
pub(crate) static WIFI_STATE_NOTIF: Notification = Notification::new();

static CONN_SIGNAL: Signal<CriticalSectionRawMutex, bool> = Signal::new();

pub async fn send<const L: usize>(topic_prefix: &str, mut mqtt: impl Client + Publish) {
    let mut connected = false;

    let topic = |topic_suffix| {
        String::<L>::from_str(topic_prefix.as_ref())
            .and_then(|mut s| s.push_str(topic_suffix).map(|_| s))
            .unwrap_or_else(|_| panic!(""))
    };

    let topic_commands = topic("/commands/#");

    let topic_valve = topic("/valve");

    let topic_meter_edges = topic("/meter/edges");
    let topic_meter_armed = topic("/meter/armed");
    let topic_meter_leak = topic("/meter/leak");

    let topic_battery_voltage = topic("/battery/voltage");
    let topic_battery_low = topic("/battery/low");
    let topic_battery_charged = topic("/battery/charged");

    let topic_powered = topic("/powered");

    let mut published_wm_state: Option<WaterMeterState> = None;
    let mut published_battery_state: Option<BatteryState> = None;

    loop {
        let (conn_state, valve_state, wm_state, battery_state) = if connected {
            match select4(
                CONN_SIGNAL.wait(),
                VALVE_STATE_NOTIF.wait(),
                WM_STATE_NOTIF.wait(),
                BATTERY_STATE_NOTIF.wait(),
            )
            .await
            {
                Either4::First(conn_state) => (Some(conn_state), None, None, None),
                Either4::Second(_) => (None, Some(valve::STATE.get()), None, None),
                Either4::Third(_) => (None, None, Some(wm::STATE.get()), None),
                Either4::Fourth(_) => (None, None, None, Some(battery::STATE.get())),
            }
        } else {
            let conn_state = CONN_SIGNAL.wait().await;

            (Some(conn_state), None, None, None)
        };

        if let Some(conn_state) = conn_state {
            if conn_state {
                info!("MQTT is now connected, subscribing");

                error::check!(
                    mqtt.subscribe(topic_commands.as_str(), QoS::AtLeastOnce)
                        .await
                )
                .unwrap();

                connected = true;
            } else {
                info!("MQTT disconnected");

                connected = false;
            }
        }

        if let Some(valve_state) = valve_state {
            let status = match valve_state {
                Some(ValveState::Open) => "open",
                Some(ValveState::Opening) => "opening",
                Some(ValveState::Closed) => "closed",
                Some(ValveState::Closing) => "closing",
                None => "unknown",
            };

            publish(
                connected,
                &mut mqtt,
                &topic_valve,
                QoS::AtLeastOnce,
                status.as_bytes(),
            )
            .await;
        }

        if let Some(wm_state) = wm_state {
            if published_wm_state
                .map(|p| p.edges_count != wm_state.edges_count)
                .unwrap_or(true)
            {
                let num = wm_state.edges_count.to_le_bytes();
                let num_slice: &[u8] = &num;

                publish(
                    connected,
                    &mut mqtt,
                    &topic_meter_edges,
                    QoS::AtLeastOnce,
                    num_slice,
                )
                .await;
            }

            if published_wm_state
                .map(|p| p.armed != wm_state.armed)
                .unwrap_or(true)
            {
                publish(
                    connected,
                    &mut mqtt,
                    &topic_meter_armed,
                    QoS::AtLeastOnce,
                    (if wm_state.armed { "true" } else { "false" }).as_bytes(),
                )
                .await;
            }

            if published_wm_state
                .map(|p| p.leaking != wm_state.leaking)
                .unwrap_or(true)
            {
                publish(
                    connected,
                    &mut mqtt,
                    &topic_meter_leak,
                    QoS::AtLeastOnce,
                    (if wm_state.armed { "true" } else { "false" }).as_bytes(),
                )
                .await;
            }

            published_wm_state = Some(wm_state);
        }

        if let Some(battery_state) = battery_state {
            if published_battery_state
                .map(|p| p.voltage != battery_state.voltage)
                .unwrap_or(true)
            {
                if let Some(voltage) = battery_state.voltage {
                    let num = voltage.to_le_bytes();
                    let num_slice: &[u8] = &num;

                    publish(
                        connected,
                        &mut mqtt,
                        &topic_battery_voltage,
                        QoS::AtMostOnce,
                        num_slice,
                    )
                    .await;

                    if let Some(prev_voltage) = published_battery_state.and_then(|p| p.voltage) {
                        if (prev_voltage > BatteryState::LOW_VOLTAGE)
                            != (voltage > BatteryState::LOW_VOLTAGE)
                        {
                            let status = if voltage > BatteryState::LOW_VOLTAGE {
                                "false"
                            } else {
                                "true"
                            };

                            publish(
                                connected,
                                &mut mqtt,
                                &topic_battery_low,
                                QoS::AtLeastOnce,
                                status.as_bytes(),
                            )
                            .await;
                        }

                        if (prev_voltage >= BatteryState::MAX_VOLTAGE)
                            != (voltage >= BatteryState::MAX_VOLTAGE)
                        {
                            let status = if voltage >= BatteryState::MAX_VOLTAGE {
                                "true"
                            } else {
                                "false"
                            };

                            publish(
                                connected,
                                &mut mqtt,
                                &topic_battery_charged,
                                QoS::AtMostOnce,
                                status.as_bytes(),
                            )
                            .await;
                        }
                    }
                }
            }

            if published_battery_state
                .map(|p| p.powered != battery_state.powered)
                .unwrap_or(true)
            {
                if let Some(powered) = battery_state.powered {
                    publish(
                        connected,
                        &mut mqtt,
                        &topic_powered,
                        QoS::AtMostOnce,
                        (if powered { "true" } else { "false" }).as_bytes(),
                    )
                    .await;
                }
            }

            published_battery_state = Some(battery_state);
        };
    }
}

async fn publish(connected: bool, mqtt: &mut impl Publish, topic: &str, qos: QoS, payload: &[u8]) {
    if connected {
        if let Ok(_msg_id) = error::check!(mqtt.publish(topic, qos, false, payload).await) {
            // TODO
            info!("Published to {}", topic);

            if qos >= QoS::AtLeastOnce {
                for notification in PUBLISH_NOTIFY {
                    notification.notify();
                }
            }
        }
    } else {
        error!("Client not connected, skipping publishment to {}", topic);
    }
}

pub async fn receive(mut connection: impl Connection<Message = Option<MqttCommand>>) {
    loop {
        let message = connection.next().await;

        if let Some(message) = message {
            info!("[MQTT/CONNECTION]: {:?}", message);

            if let Ok(Event::Received(Some(cmd))) = &message {
                match cmd {
                    MqttCommand::Valve(open) => {
                        valve::COMMAND.signal(if *open {
                            ValveCommand::Open
                        } else {
                            ValveCommand::Close
                        });
                    }
                    MqttCommand::FlowWatch(enable) => {
                        wm::COMMAND.signal(if *enable {
                            WaterMeterCommand::Arm
                        } else {
                            WaterMeterCommand::Disarm
                        });
                    }
                    _ => (),
                }
            } else if matches!(&message, Ok(Event::Connected(_))) {
                CONN_SIGNAL.signal(true);
            } else if matches!(&message, Ok(Event::Disconnected)) {
                CONN_SIGNAL.signal(false);
            }

            for notification in RECEIVE_NOTIFY {
                notification.notify();
            }
        } else {
            break;
        }
    }
}

#[derive(Default)]
pub struct MessageParser {
    #[allow(clippy::type_complexity)]
    command_parser: Option<fn(&[u8]) -> Option<MqttCommand>>,
    payload_buf: [u8; 16],
}

impl MessageParser {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn convert<M, E>(
        &mut self,
        event: &Result<Event<M>, E>,
    ) -> Result<Event<Option<MqttCommand>>, E>
    where
        M: Message,
        E: Clone,
    {
        event
            .as_ref()
            .map(|event| event.transform_received(|message| self.process(message)))
            .map_err(|e| e.clone())
    }

    fn process<M>(&mut self, message: &M) -> Option<MqttCommand>
    where
        M: Message,
    {
        match message.details() {
            Details::Complete => Self::parse_command(message.topic().unwrap())
                .and_then(|parser| parser(message.data())),
            Details::InitialChunk(initial_chunk_data) => {
                if initial_chunk_data.total_data_size > self.payload_buf.len() {
                    self.command_parser = None;
                } else {
                    self.command_parser = Self::parse_command(message.topic().unwrap());

                    self.payload_buf[..message.data().len()]
                        .copy_from_slice(message.data().as_ref());
                }

                None
            }
            Details::SubsequentChunk(subsequent_chunk_data) => {
                if let Some(command_parser) = self.command_parser.as_ref() {
                    self.payload_buf
                        [subsequent_chunk_data.current_data_offset..message.data().len()]
                        .copy_from_slice(message.data().as_ref());

                    if subsequent_chunk_data.total_data_size
                        == subsequent_chunk_data.current_data_offset + message.data().len()
                    {
                        command_parser(&self.payload_buf[0..subsequent_chunk_data.total_data_size])
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        }
    }

    #[allow(clippy::type_complexity)]
    fn parse_command(topic: &str) -> Option<fn(&[u8]) -> Option<MqttCommand>> {
        if topic.ends_with("/commands/valve") {
            Some(Self::parse_valve_command)
        } else if topic.ends_with("/commands/flow_watch") {
            Some(Self::parse_flow_watch_command)
        } else if topic.ends_with("/commands/keep_alive") {
            Some(Self::parse_keep_alive_command)
        } else if topic.ends_with("/commands/system_update") {
            Some(Self::parse_system_update_command)
        } else {
            None
        }
    }

    fn parse_valve_command(data: &[u8]) -> Option<MqttCommand> {
        Self::parse::<bool>(data).map(MqttCommand::Valve)
    }

    fn parse_flow_watch_command(data: &[u8]) -> Option<MqttCommand> {
        Self::parse::<bool>(data).map(MqttCommand::FlowWatch)
    }

    fn parse_keep_alive_command(data: &[u8]) -> Option<MqttCommand> {
        Self::parse::<u32>(data).map(|secs| MqttCommand::KeepAlive(Duration::from_secs(secs as _)))
    }

    fn parse_system_update_command(data: &[u8]) -> Option<MqttCommand> {
        Self::parse_empty(data).map(|_| MqttCommand::SystemUpdate)
    }

    fn parse<T>(data: &[u8]) -> Option<T>
    where
        T: str::FromStr,
    {
        str::from_utf8(data)
            .ok()
            .and_then(|s| str::parse::<T>(s).ok())
    }

    fn parse_empty(data: &[u8]) -> Option<()> {
        if data.is_empty() {
            Some(())
        } else {
            None
        }
    }
}
