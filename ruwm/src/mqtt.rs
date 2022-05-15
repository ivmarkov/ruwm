use core::str;
use core::time::Duration;

extern crate alloc;
use alloc::format;

use embedded_svc::utils::asyncs::channel::adapt::both;
use log::info;

use serde::{Deserialize, Serialize};

use embedded_svc::channel::asyncs::{Receiver, Sender};
use embedded_svc::mqtt::client::asyncs::{
    Client, Connection, Event, Message, MessageId, Publish, QoS,
};
use embedded_svc::mqtt::client::Details;
use embedded_svc::mutex::MutexFamily;
use embedded_svc::signal::asyncs::{SendSyncSignalFamily, Signal};
use embedded_svc::utils::asyncs::select::{select4, Either4};
use embedded_svc::utils::asyncs::signal::adapt::as_sender;

use crate::battery::BatteryState;
use crate::error;
use crate::utils::{as_static_receiver, as_static_sender};
use crate::valve::{ValveCommand, ValveState};
use crate::water_meter::{WaterMeterCommand, WaterMeterState};

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub enum MqttCommand {
    KeepAlive(Duration),
    Valve(bool),
    FlowWatch(bool),
    SystemUpdate,
}

pub type MqttClientNotification = Result<Event<Option<MqttCommand>>, ()>;

pub struct Mqtt<M>
where
    M: MutexFamily + SendSyncSignalFamily,
{
    pub_signal: M::Signal<MessageId>, // TODO: Not clear if a signal is a good fit
    notif_signal: M::Signal<MqttClientNotification>, // TODO: Not clear if a signal is a good fit
    valve_state_signal: M::Signal<Option<ValveState>>,
    wm_state_signal: M::Signal<WaterMeterState>,
    battery_state_signal: M::Signal<BatteryState>,
}

impl<M> Mqtt<M>
where
    M: MutexFamily + SendSyncSignalFamily,
{
    pub fn new() -> Self {
        Self {
            pub_signal: M::Signal::new(),
            notif_signal: M::Signal::new(),
            valve_state_signal: M::Signal::new(),
            wm_state_signal: M::Signal::new(),
            battery_state_signal: M::Signal::new(),
        }
    }

    pub fn valve_state_sink(&self) -> impl Sender<Data = Option<ValveState>> + '_ {
        as_sender(&self.valve_state_signal)
    }

    pub fn wm_state_sink(&self) -> impl Sender<Data = WaterMeterState> + '_ {
        as_sender(&self.wm_state_signal)
    }

    pub fn battery_state_sink(&'static self) -> impl Sender<Data = BatteryState> + 'static {
        as_sender(&self.battery_state_signal)
    }

    pub async fn send(
        &'static self,
        topic_prefix: impl AsRef<str>,
        mqtt: impl Client + Publish,
        pub_sink: impl Sender<Data = MessageId> + Send + 'static,
    ) -> error::Result<()> {
        send(
            topic_prefix,
            mqtt,
            as_static_receiver(&self.notif_signal),
            as_static_receiver(&self.valve_state_signal),
            as_static_receiver(&self.wm_state_signal),
            as_static_receiver(&self.battery_state_signal),
            both(as_static_sender(&self.pub_signal), pub_sink),
        )
        .await
    }

    pub async fn receive(
        &'static self,
        connection: impl Connection<Message = Option<MqttCommand>>,
        notif_sink: impl Sender<Data = MqttClientNotification> + Send + 'static,
        valve_command_sink: impl Sender<Data = ValveCommand>,
        wm_command_sink: impl Sender<Data = WaterMeterCommand>,
    ) -> error::Result<()> {
        receive(
            connection,
            both(as_static_sender(&self.notif_signal), notif_sink),
            valve_command_sink,
            wm_command_sink,
        )
        .await
    }
}

pub async fn send(
    topic_prefix: impl AsRef<str>,
    mut mqtt: impl Client + Publish,
    mut notif_source: impl Receiver<Data = MqttClientNotification>,
    mut valve_state_source: impl Receiver<Data = Option<ValveState>>,
    mut wm_state_source: impl Receiver<Data = WaterMeterState>,
    mut battery_state_source: impl Receiver<Data = BatteryState>,
    mut pub_sink: impl Sender<Data = MessageId>,
) -> error::Result<()> {
    let mut connected = false;

    let topic_prefix = topic_prefix.as_ref();

    let topic_valve = format!("{}/valve", topic_prefix);

    let topic_meter_edges = format!("{}/meter/edges", topic_prefix);
    let topic_meter_armed = format!("{}/meter/armed", topic_prefix);
    let topic_meter_leak = format!("{}/meter/leak", topic_prefix);

    let topic_battery_voltage = format!("{}/battery/voltage", topic_prefix);
    let topic_battery_low = format!("{}/battery/low", topic_prefix);
    let topic_battery_charged = format!("{}/battery/charged", topic_prefix);

    let topic_powered = format!("{}/powered", topic_prefix);

    loop {
        let (mqtt_state, valve_state, wm_state, battery_state) = {
            let mqtt = notif_source.recv();
            let valve = valve_state_source.recv();
            let wm = wm_state_source.recv();
            let battery = battery_state_source.recv();

            //pin_mut!(mqtt, valve, wm, battery);

            match select4(mqtt, valve, wm, battery).await {
                Either4::First(mqtt_state) => {
                    (Some(mqtt_state.map_err(error::svc)?), None, None, None)
                }
                Either4::Second(valve_state) => {
                    (None, Some(valve_state.map_err(error::svc)?), None, None)
                }
                Either4::Third(wm_state) => (None, None, Some(wm_state.map_err(error::svc)?), None),
                Either4::Fourth(battery_state) => {
                    (None, None, None, Some(battery_state.map_err(error::svc)?))
                }
            }
        };

        if let Some(mqtt_state) = mqtt_state {
            match mqtt_state {
                Ok(Event::Connected(_)) => {
                    error::check!(mqtt
                        .subscribe(format!("{}/commands/#", topic_prefix), QoS::AtLeastOnce)
                        .await
                        .map_err(error::svc));

                    connected = true;
                }
                Ok(Event::Disconnected) => {
                    connected = false;
                }
                _ => {}
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
                &mut pub_sink,
                &topic_valve,
                QoS::AtLeastOnce,
                status.as_bytes(),
            )
            .await?;
        }

        if let Some(wm_state) = wm_state {
            if wm_state.prev_edges_count != wm_state.edges_count {
                let num = wm_state.edges_count.to_le_bytes();
                let num_slice: &[u8] = &num;

                publish(
                    connected,
                    &mut mqtt,
                    &mut pub_sink,
                    &topic_meter_edges,
                    QoS::AtLeastOnce,
                    num_slice,
                )
                .await?;
            }

            if wm_state.prev_armed != wm_state.armed {
                publish(
                    connected,
                    &mut mqtt,
                    &mut pub_sink,
                    &topic_meter_armed,
                    QoS::AtLeastOnce,
                    (if wm_state.armed { "true" } else { "false" }).as_bytes(),
                )
                .await?;
            }

            if wm_state.prev_leaking != wm_state.leaking {
                publish(
                    connected,
                    &mut mqtt,
                    &mut pub_sink,
                    &topic_meter_leak,
                    QoS::AtLeastOnce,
                    (if wm_state.armed { "true" } else { "false" }).as_bytes(),
                )
                .await?;
            }
        }

        if let Some(battery_state) = battery_state {
            if battery_state.prev_voltage != battery_state.voltage {
                if let Some(voltage) = battery_state.voltage {
                    let num = voltage.to_le_bytes();
                    let num_slice: &[u8] = &num;

                    publish(
                        connected,
                        &mut mqtt,
                        &mut pub_sink,
                        &topic_battery_voltage,
                        QoS::AtMostOnce,
                        num_slice,
                    )
                    .await?;

                    if let Some(prev_voltage) = battery_state.prev_voltage {
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
                                &mut pub_sink,
                                &topic_battery_low,
                                QoS::AtLeastOnce,
                                status.as_bytes(),
                            )
                            .await?;
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
                                &mut pub_sink,
                                &topic_battery_charged,
                                QoS::AtMostOnce,
                                status.as_bytes(),
                            )
                            .await?;
                        }
                    }
                }
            }

            if battery_state.prev_powered != battery_state.powered {
                if let Some(powered) = battery_state.powered {
                    publish(
                        connected,
                        &mut mqtt,
                        &mut pub_sink,
                        &topic_powered,
                        QoS::AtMostOnce,
                        (if powered { "true" } else { "false" }).as_bytes(),
                    )
                    .await?;
                }
            }
        };
    }
}

async fn publish(
    connected: bool,
    mqtt: &mut impl Publish,
    pub_sink: &mut impl Sender<Data = MessageId>,
    topic: &str,
    qos: QoS,
    payload: &[u8],
) -> error::Result<()> {
    if connected {
        if let Some(msg_id) = error::check!(mqtt
            .publish(topic, qos, false, payload)
            .await
            .map_err(error::svc))
        {
            info!("Published to {}", topic);

            if qos >= QoS::AtLeastOnce {
                pub_sink.send(msg_id).await.map_err(error::svc)?;
            }
        }
    } else {
        info!("Client not connected, skipping publishment to {}", topic);
    }

    Ok(())
}

pub async fn receive(
    mut connection: impl Connection<Message = Option<MqttCommand>>,
    mut notif_sink: impl Sender<Data = MqttClientNotification>,
    mut valve_command_sink: impl Sender<Data = ValveCommand>,
    mut wm_command_sink: impl Sender<Data = WaterMeterCommand>,
) -> error::Result<()> {
    loop {
        let message = connection.next().await;

        if let Some(message) = message {
            if let Ok(Event::Received(Some(cmd))) = &message {
                match cmd {
                    MqttCommand::Valve(open) => valve_command_sink
                        .send(if *open {
                            ValveCommand::Open
                        } else {
                            ValveCommand::Close
                        })
                        .await
                        .map_err(error::svc)?,
                    MqttCommand::FlowWatch(enable) => wm_command_sink
                        .send(if *enable {
                            WaterMeterCommand::Arm
                        } else {
                            WaterMeterCommand::Disarm
                        })
                        .await
                        .map_err(error::svc)?,
                    _ => (),
                }
            }

            notif_sink
                .send(message.map_err(|_| ()))
                .await
                .map_err(error::svc)?;
        } else {
            break;
        }
    }

    Ok(())
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
            Details::Complete => Self::parse_command(message.topic().unwrap().as_ref())
                .and_then(|parser| parser(message.data().as_ref())),
            Details::InitialChunk(initial_chunk_data) => {
                if initial_chunk_data.total_data_size > self.payload_buf.len() {
                    self.command_parser = None;
                } else {
                    self.command_parser = Self::parse_command(message.topic().unwrap().as_ref());

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
