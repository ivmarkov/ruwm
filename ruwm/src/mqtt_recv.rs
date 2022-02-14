use core::fmt::{Debug, Display};
use core::ops::Deref;
use core::result::Result;
use core::str;
use core::time::Duration;

extern crate alloc;
use alloc::format;

use embedded_svc::channel::nonblocking::Sender;
use embedded_svc::mqtt::client::nonblocking::Connection;
use embedded_svc::mqtt::client::nonblocking::{Client, Details, Event, Message, MessageId, QoS};

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum MqttCommand {
    KeepAlive(Duration),
    Valve(bool),
    FlowWatch(bool),
    SystemUpdate,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum MqttClientNotification {
    BeforeConnect,
    Connected(bool),
    Disconnected,
    Subscribed(MessageId),
    Unsubscribed(MessageId),
    Published(MessageId),
    Deleted(MessageId),
    Received(MessageId),
    Error, // TODO
}

impl<M, E> From<&Result<Event<M>, E>> for MqttClientNotification
where
    M: Message,
    E: Display,
{
    fn from(value: &Result<Event<M>, E>) -> Self {
        match value {
            Ok(event) => match event {
                Event::BeforeConnect => MqttClientNotification::BeforeConnect,
                Event::Connected(connected) => MqttClientNotification::Connected(*connected),
                Event::Disconnected => MqttClientNotification::Disconnected,
                Event::Subscribed(id) => MqttClientNotification::Subscribed(*id),
                Event::Unsubscribed(id) => MqttClientNotification::Unsubscribed(*id),
                Event::Published(id) => MqttClientNotification::Published(*id),
                Event::Received(message) => MqttClientNotification::Received(message.id()),
                Event::Deleted(id) => MqttClientNotification::Deleted(*id),
            },
            Err(_) => MqttClientNotification::Error,
        }
    }
}

pub async fn run<C, M, Q, S>(
    mut mqttc: C,
    topic_prefix: &str,
    mut mqtt: M,
    mut state: Q,
    mut command: S,
) where
    C: Client,
    M: Connection,
    M::Error: Display,
    Q: Sender<Data = MqttClientNotification>,
    S: Sender<Data = MqttCommand>,
{
    mqttc
        .subscribe(format!("{}/commands/#", topic_prefix), QoS::AtLeastOnce)
        .await
        .unwrap();

    let mut message_parser = MessageParser::new();

    loop {
        let incoming_ref = mqtt.next().await;
        let incoming = incoming_ref.deref();

        if let Some(incoming) = incoming {
            let notification = MqttClientNotification::from(incoming);
            state.send(notification).await.unwrap();

            if let Ok(Event::Received(message)) = incoming {
                if let Some(cmd) = message_parser.process(message) {
                    command.send(cmd).await.unwrap();
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
