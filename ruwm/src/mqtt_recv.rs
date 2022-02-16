use core::fmt::{Debug, Display};
use core::result::Result;
use core::str;
use core::time::Duration;

use embedded_svc::channel::nonblocking::Sender;
use embedded_svc::mqtt::client::nonblocking::Connection;
use embedded_svc::mqtt::client::nonblocking::{Details, Event, Message};

use crate::valve::ValveCommand;
use crate::water_meter::WaterMeterCommand;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum MqttCommand {
    KeepAlive(Duration),
    Valve(bool),
    FlowWatch(bool),
    SystemUpdate,
}

pub type MqttClientNotification = Result<Event<Option<MqttCommand>>, ()>;

pub async fn run<M, Q, SV, SW>(mut mqtt: M, mut state: Q, mut valve_command: SV, mut wm_command: SW)
where
    M: Connection,
    M::Error: Display,
    Q: Sender<Data = MqttClientNotification>,
    SV: Sender<Data = ValveCommand>,
    SW: Sender<Data = WaterMeterCommand>,
{
    let mut message_parser = MessageParser::new();

    loop {
        let incoming = mqtt.next(
            |message| message_parser.process(message),
                |error| (),
        ).await;
        
        if let Some(incoming) = incoming {
            state.send(incoming.clone()).await.unwrap();

            if let Ok(Event::Received(Some(cmd))) = incoming {
                match cmd {
                    MqttCommand::Valve(open) => valve_command.send(if open { ValveCommand::Open} else { ValveCommand::Close }).await.unwrap(),
                    MqttCommand::FlowWatch(enable) => wm_command.send(if enable { WaterMeterCommand::Arm } else { WaterMeterCommand::Disarm }).await.unwrap(),
                    _ => (),
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
