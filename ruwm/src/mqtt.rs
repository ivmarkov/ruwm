extern crate alloc;
use core::fmt::Display;
use core::str;
use core::time::Duration;

use alloc::format;

use embedded_svc::mqtt::client::Details;
use futures::future::{join, select, Either};
use futures::pin_mut;

use embedded_svc::channel::nonblocking::{Receiver, Sender};
use embedded_svc::mqtt::client::nonblocking::{
    Client, Connection, Event, Message, MessageId, Publish, QoS,
};
use log::info;

use crate::battery::BatteryState;
use crate::valve::{ValveCommand, ValveState};
use crate::water_meter::{WaterMeterCommand, WaterMeterState};

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum MqttCommand {
    KeepAlive(Duration),
    Valve(bool),
    FlowWatch(bool),
    SystemUpdate,
}

pub type MqttClientNotification = Result<Event<Option<MqttCommand>>, ()>;

pub struct Mqtt<C, M, Q, V, W, B, N, SV, SW> {
    sender: MqttSender<C, Q, V, W, B>,
    receiver: MqttReceiver<M, N, SV, SW>,
}

impl<C, M, Q, V, W, B, N, SV, SW> Mqtt<C, M, Q, V, W, B, N, SV, SW>
where
    C: Client + Publish,
    M: Connection,
    M::Error: Display,
    Q: Sender<Data = MessageId>,
    V: Receiver<Data = Option<ValveState>>,
    W: Receiver<Data = WaterMeterState>,
    B: Receiver<Data = BatteryState>,
    N: Sender<Data = MqttClientNotification>,
    SV: Sender<Data = ValveCommand>,
    SW: Sender<Data = WaterMeterCommand>,
{
    pub fn new(
        mqtt: C,
        connection: M,
        pubq: Q,
        valve_status: V,
        wm_status: W,
        battery_status: B,
        mqtt_notif: N,
        valve_command: SV,
        wm_command: SW,
    ) -> Self {
        Self {
            sender: MqttSender::new(mqtt, pubq, valve_status, wm_status, battery_status),
            receiver: MqttReceiver::new(connection, mqtt_notif, valve_command, wm_command),
        }
    }

    pub async fn run(&mut self, topic_prefix: impl AsRef<str>) {
        join(self.sender.run(topic_prefix), self.receiver.run()).await;
    }
}

struct MqttSender<C, Q, V, W, B> {
    mqtt: C,
    pubq: Q,
    valve_status: V,
    wm_status: W,
    battery_status: B,
}

impl<C, Q, V, W, B> MqttSender<C, Q, V, W, B>
where
    C: Client + Publish,
    Q: Sender<Data = MessageId>,
    V: Receiver<Data = Option<ValveState>>,
    W: Receiver<Data = WaterMeterState>,
    B: Receiver<Data = BatteryState>,
{
    fn new(mqtt: C, pubq: Q, valve_status: V, wm_status: W, battery_status: B) -> Self {
        Self {
            mqtt,
            pubq,
            valve_status,
            wm_status,
            battery_status,
        }
    }

    async fn run(&mut self, topic_prefix: impl AsRef<str>) {
        let topic_prefix = topic_prefix.as_ref();

        self.mqtt
            .subscribe(format!("{}/commands/#", topic_prefix), QoS::AtLeastOnce)
            .await
            .unwrap();

        let topic_valve = format!("{}/valve", topic_prefix);

        let topic_meter_edges = format!("{}/meter/edges", topic_prefix);
        let topic_meter_armed = format!("{}/meter/armed", topic_prefix);
        let topic_meter_leak = format!("{}/meter/leak", topic_prefix);

        let topic_battery_voltage = format!("{}/battery/voltage", topic_prefix);
        let topic_battery_low = format!("{}/battery/low", topic_prefix);
        let topic_battery_charged = format!("{}/battery/charged", topic_prefix);

        let topic_powered = format!("{}/powered", topic_prefix);

        loop {
            let (valve_state, wm_state, battery_state) = {
                let valve = self.valve_status.recv();
                let wm = self.wm_status.recv();
                let battery = self.battery_status.recv();

                pin_mut!(valve);
                pin_mut!(wm);
                pin_mut!(battery);

                match select(valve, select(wm, battery)).await {
                    Either::Left((valve_state, _)) => (Some(valve_state.unwrap()), None, None),
                    Either::Right((Either::Left((wm_state, _)), _)) => {
                        (None, Some(wm_state.unwrap()), None)
                    }
                    Either::Right((Either::Right((battery_state, _)), _)) => {
                        (None, None, Some(battery_state.unwrap()))
                    }
                }
            };

            if let Some(valve_state) = valve_state {
                let status = match valve_state {
                    Some(ValveState::Open) => "open",
                    Some(ValveState::Opening) => "opening",
                    Some(ValveState::Closed) => "closed",
                    Some(ValveState::Closing) => "closing",
                    None => "unknown",
                };

                self.publish(&topic_valve, QoS::AtLeastOnce, status.as_bytes())
                    .await;
            }

            if let Some(wm_state) = wm_state {
                if wm_state.prev_edges_count != wm_state.edges_count {
                    let num = wm_state.edges_count.to_le_bytes();
                    let num_slice: &[u8] = &num;

                    self.publish(&topic_meter_edges, QoS::AtLeastOnce, num_slice)
                        .await;
                }

                if wm_state.prev_armed != wm_state.armed {
                    self.publish(
                        &topic_meter_armed,
                        QoS::AtLeastOnce,
                        (if wm_state.armed { "true" } else { "false" }).as_bytes(),
                    )
                    .await;
                }

                if wm_state.prev_leaking != wm_state.leaking {
                    self.publish(
                        &topic_meter_leak,
                        QoS::AtLeastOnce,
                        (if wm_state.armed { "true" } else { "false" }).as_bytes(),
                    )
                    .await;
                }
            }

            if let Some(battery_state) = battery_state {
                if battery_state.prev_voltage != battery_state.voltage {
                    if let Some(voltage) = battery_state.voltage {
                        let num = voltage.to_le_bytes();
                        let num_slice: &[u8] = &num;

                        self.publish(&topic_battery_voltage, QoS::AtMostOnce, num_slice)
                            .await;

                        if let Some(prev_voltage) = battery_state.prev_voltage {
                            if (prev_voltage > BatteryState::LOW_VOLTAGE)
                                != (voltage > BatteryState::LOW_VOLTAGE)
                            {
                                let status = if voltage > BatteryState::LOW_VOLTAGE {
                                    "false"
                                } else {
                                    "true"
                                };

                                self.publish(
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

                                self.publish(
                                    &topic_battery_charged,
                                    QoS::AtMostOnce,
                                    status.as_bytes(),
                                )
                                .await;
                            }
                        }
                    }
                }

                if battery_state.prev_powered != battery_state.powered {
                    if let Some(powered) = battery_state.powered {
                        self.publish(
                            &topic_powered,
                            QoS::AtMostOnce,
                            (if powered { "true" } else { "false" }).as_bytes(),
                        )
                        .await;
                    }
                }
            };
        }
    }

    async fn publish(&mut self, topic: &str, qos: QoS, payload: &[u8]) {
        let msg_id = self.mqtt.publish(topic, qos, false, payload).await.unwrap();

        info!("Published to {}", topic);

        if qos >= QoS::AtLeastOnce {
            self.pubq.send(msg_id).await.unwrap();
        }
    }
}

pub struct MqttReceiver<M, N, SV, SW> {
    connection: M,
    mqtt_notif: N,
    valve_command: SV,
    wm_command: SW,
}

impl<M, N, SV, SW> MqttReceiver<M, N, SV, SW>
where
    M: Connection,
    M::Error: Display,
    N: Sender<Data = MqttClientNotification>,
    SV: Sender<Data = ValveCommand>,
    SW: Sender<Data = WaterMeterCommand>,
{
    fn new(connection: M, mqtt_notif: N, valve_command: SV, wm_command: SW) -> Self {
        Self {
            connection,
            mqtt_notif,
            valve_command,
            wm_command,
        }
    }

    async fn run(&mut self) {
        let mut message_parser = MessageParser::new();

        loop {
            let incoming = self
                .connection
                .next(|message| message_parser.process(message), |error| ())
                .await;

            if let Some(incoming) = incoming {
                self.mqtt_notif.send(incoming.clone()).await.unwrap();

                if let Ok(Event::Received(Some(cmd))) = incoming {
                    match cmd {
                        MqttCommand::Valve(open) => self
                            .valve_command
                            .send(if open {
                                ValveCommand::Open
                            } else {
                                ValveCommand::Close
                            })
                            .await
                            .unwrap(),
                        MqttCommand::FlowWatch(enable) => self
                            .wm_command
                            .send(if enable {
                                WaterMeterCommand::Arm
                            } else {
                                WaterMeterCommand::Disarm
                            })
                            .await
                            .unwrap(),
                        _ => (),
                    }
                }
            }
        }
    }
}

#[derive(Default)]
struct MessageParser {
    command_parser: Option<fn(&[u8]) -> Option<MqttCommand>>,
    payload_buf: [u8; 16],
}

impl MessageParser {
    fn new() -> Self {
        Default::default()
    }

    fn process<M>(&mut self, message: &M) -> Option<MqttCommand>
    where
        M: Message,
    {
        match message.details() {
            Details::Complete(topic_token) => {
                Self::parse_command(message.topic(topic_token).as_ref())
                    .and_then(|parser| parser(message.data().as_ref()))
            }
            Details::InitialChunk(initial_chunk_data) => {
                if initial_chunk_data.total_data_size > self.payload_buf.len() {
                    self.command_parser = None;
                } else {
                    self.command_parser = Self::parse_command(
                        message.topic(&initial_chunk_data.topic_token).as_ref(),
                    );

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
        Self::parse::<bool>(data).map(|flag| MqttCommand::Valve(flag))
    }

    fn parse_flow_watch_command(data: &[u8]) -> Option<MqttCommand> {
        Self::parse::<bool>(data).map(|flag| MqttCommand::FlowWatch(flag))
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
