use core::str::{self, FromStr};
use core::time::Duration;

use log::{error, info};

use serde::{Deserialize, Serialize};

use heapless::String;

use embassy_futures::select::{select4, Either4};
use embassy_sync::blocking_mutex::raw::RawMutex;

use embedded_svc::mqtt::client::asynch::{
    Client, Connection, Event, Message, MessageId, Publish, QoS,
};
use embedded_svc::mqtt::client::Details;

use crate::battery::BatteryState;
use crate::channel::{Receiver, Sender};
use crate::error;
use crate::notification::Notification;
use crate::signal::Signal;
use crate::state::StateCellRead;
use crate::utils::{NotifReceiver, SignalReceiver, SignalSender};
use crate::valve::{ValveCommand, ValveState};
use crate::water_meter::{WaterMeterCommand, WaterMeterState};

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

pub struct Mqtt<R>
where
    R: RawMutex,
{
    conn_signal: Signal<R, bool>,
    valve_state_notif: Notification,
    wm_state_notif: Notification,
    battery_state_notif: Notification,
}

impl<R> Mqtt<R>
where
    R: RawMutex + Send + Sync + 'static,
{
    pub const fn new() -> Self {
        Self {
            conn_signal: Signal::new(),
            valve_state_notif: Notification::new(),
            wm_state_notif: Notification::new(),
            battery_state_notif: Notification::new(),
        }
    }

    pub fn valve_state_sink(&self) -> &Notification {
        &self.valve_state_notif
    }

    pub fn wm_state_sink(&'static self) -> &Notification {
        &self.wm_state_notif
    }

    pub fn battery_state_sink(&'static self) -> &Notification {
        &self.battery_state_notif
    }

    pub async fn send<const L: usize>(
        &'static self,
        topic_prefix: impl AsRef<str>,
        mqtt: impl Client + Publish,
        valve_state: &'static (impl StateCellRead<Data = Option<ValveState>> + Send + Sync + 'static),
        wm_state: &'static (impl StateCellRead<Data = WaterMeterState> + Send + Sync + 'static),
        battery_state: &'static (impl StateCellRead<Data = BatteryState> + Send + Sync + 'static),
        pub_sink: impl Sender<Data = MessageId> + Send + 'static,
    ) {
        send::<_, L>(
            topic_prefix,
            mqtt,
            SignalReceiver::new(&self.conn_signal),
            NotifReceiver::new(&self.valve_state_notif, valve_state),
            NotifReceiver::new(&self.wm_state_notif, wm_state),
            NotifReceiver::new(&self.battery_state_notif, battery_state),
            pub_sink,
        )
        .await
        .unwrap(); // TODO
    }

    pub async fn receive(
        &'static self,
        connection: impl Connection<Message = Option<MqttCommand>>,
        notif_sink: impl Sender<Data = MqttClientNotification> + 'static,
        valve_command_sink: impl Sender<Data = ValveCommand> + 'static,
        wm_command_sink: impl Sender<Data = WaterMeterCommand> + 'static,
    ) {
        receive(
            connection,
            SignalSender::new("MQTT/CONNECTION", [&self.conn_signal]),
            notif_sink,
            valve_command_sink,
            wm_command_sink,
        )
        .await
    }
}

pub async fn send<M, const L: usize>(
    topic_prefix: impl AsRef<str>,
    mut mqtt: M,
    mut conn_source: impl Receiver<Data = bool>,
    mut valve_state_source: impl Receiver<Data = Option<ValveState>>,
    mut wm_state_source: impl Receiver<Data = WaterMeterState>,
    mut battery_state_source: impl Receiver<Data = BatteryState>,
    mut pub_sink: impl Sender<Data = MessageId>,
) -> Result<(), M::Error>
where
    M: Client + Publish,
{
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
            let conn = conn_source.recv();
            let valve = valve_state_source.recv();
            let wm = wm_state_source.recv();
            let battery = battery_state_source.recv();

            //pin_mut!(conn, valve, wm, battery);

            match select4(conn, valve, wm, battery).await {
                Either4::First(conn_state) => (Some(conn_state), None, None, None),
                Either4::Second(valve_state) => (None, Some(valve_state), None, None),
                Either4::Third(wm_state) => (None, None, Some(wm_state), None),
                Either4::Fourth(battery_state) => (None, None, None, Some(battery_state)),
            }
        } else {
            let conn_state = conn_source.recv().await;

            (Some(conn_state), None, None, None)
        };

        if let Some(conn_state) = conn_state {
            if conn_state {
                info!("MQTT is now connected, subscribing");

                error::check!(
                    mqtt.subscribe(topic_commands.as_str(), QoS::AtLeastOnce)
                        .await
                )?;

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
                &mut pub_sink,
                &topic_valve,
                QoS::AtLeastOnce,
                status.as_bytes(),
            )
            .await?;
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
                    &mut pub_sink,
                    &topic_meter_edges,
                    QoS::AtLeastOnce,
                    num_slice,
                )
                .await?;
            }

            if published_wm_state
                .map(|p| p.armed != wm_state.armed)
                .unwrap_or(true)
            {
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

            if published_wm_state
                .map(|p| p.leaking != wm_state.leaking)
                .unwrap_or(true)
            {
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
                        &mut pub_sink,
                        &topic_battery_voltage,
                        QoS::AtMostOnce,
                        num_slice,
                    )
                    .await?;

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

            if published_battery_state
                .map(|p| p.powered != battery_state.powered)
                .unwrap_or(true)
            {
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

            published_battery_state = Some(battery_state);
        };
    }
}

async fn publish<M>(
    connected: bool,
    mqtt: &mut M,
    pub_sink: &mut impl Sender<Data = MessageId>,
    topic: &str,
    qos: QoS,
    payload: &[u8],
) -> Result<(), M::Error>
where
    M: Publish,
{
    if connected {
        if let Ok(msg_id) = error::check!(mqtt.publish(topic, qos, false, payload).await) {
            info!("Published to {}", topic);

            if qos >= QoS::AtLeastOnce {
                pub_sink.send(msg_id).await;
            }
        }
    } else {
        error!("Client not connected, skipping publishment to {}", topic);
    }

    Ok(())
}

pub async fn receive(
    mut connection: impl Connection<Message = Option<MqttCommand>>,
    mut conn_sink: impl Sender<Data = bool>,
    mut notif_sink: impl Sender<Data = MqttClientNotification>,
    mut valve_command_sink: impl Sender<Data = ValveCommand>,
    mut wm_command_sink: impl Sender<Data = WaterMeterCommand>,
) {
    loop {
        let message = connection.next().await;

        if let Some(message) = message {
            if let Ok(Event::Received(Some(cmd))) = &message {
                match cmd {
                    MqttCommand::Valve(open) => {
                        valve_command_sink
                            .send(if *open {
                                ValveCommand::Open
                            } else {
                                ValveCommand::Close
                            })
                            .await;
                    }
                    MqttCommand::FlowWatch(enable) => {
                        wm_command_sink
                            .send(if *enable {
                                WaterMeterCommand::Arm
                            } else {
                                WaterMeterCommand::Disarm
                            })
                            .await;
                    }
                    _ => (),
                }
            } else if matches!(&message, Ok(Event::Connected(_))) {
                conn_sink.send(true);
            } else if matches!(&message, Ok(Event::Disconnected)) {
                conn_sink.send(false);
            }

            notif_sink
                .send(message.map_err(|_| ())) // TODO
                .await;
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
