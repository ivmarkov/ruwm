use core::fmt::{Debug, Display};
use core::time::Duration;

extern crate alloc;

use embedded_svc::event_bus;
use embedded_svc::mqtt;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum MqttCommand {
    KeepAlive(Duration),
    Valve(bool),
    FlowWatch(bool),
    SystemUpdate,
}

#[derive(Default)]
struct MessageParser {
    command_parser: Option<fn(&[u8]) -> Option<MqttCommand>>,
    payload_buf: [u8; 16],
}

impl MessageParser {
    fn process<M>(&mut self, message: &M) -> Option<MqttCommand>
    where
        M: mqtt::client::Message,
    {
        match message.details() {
            mqtt::client::Details::Complete(topic_token) => {
                Self::parse_command(message.topic(topic_token).as_ref())
                    .and_then(|parser| parser(message.data().as_ref()))
            }
            mqtt::client::Details::InitialChunk(initial_chunk_data) => {
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
            mqtt::client::Details::SubsequentChunk(subsequent_chunk_data) => {
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
        Self::parse_flag(data).map(|flag| MqttCommand::Valve(flag))
    }

    fn parse_flow_watch_command(data: &[u8]) -> Option<MqttCommand> {
        Self::parse_flag(data).map(|flag| MqttCommand::FlowWatch(flag))
    }

    fn parse_keep_alive_command(data: &[u8]) -> Option<MqttCommand> {
        Self::parse_number(data).map(|secs| MqttCommand::KeepAlive(Duration::from_secs(secs as _)))
    }

    fn parse_system_update_command(data: &[u8]) -> Option<MqttCommand> {
        Self::parse_empty(data).map(|_| MqttCommand::SystemUpdate)
    }

    fn parse_flag(data: &[u8]) -> Option<bool> {
        todo!()
    }

    fn parse_number(data: &[u8]) -> Option<u32> {
        todo!()
    }

    fn parse_empty(data: &[u8]) -> Option<()> {
        if data.is_empty() {
            Some(())
        } else {
            None
        }
    }
}

pub struct MqttCommands<P> {
    poster: P,
    message_parser: MessageParser,
}

impl<P> MqttCommands<P>
where
    P: event_bus::Poster,
{
    pub const EVENT_SOURCE: event_bus::Source<MqttCommand> =
        event_bus::Source::new("MQTT_COMMANDS");

    pub fn callback<'a, C, M>(
        client: &mut C,
        poster: P,
    ) -> anyhow::Result<impl FnMut(&'a mqtt::client::Event<M>) -> anyhow::Result<()>>
    where
        C: mqtt::client::Client,
        M: mqtt::client::Message,
        C::Error: Debug + Display + Send + Sync + 'static,
    {
        let mut this = Self::new::<_, M>(client, poster)?;

        Ok(move |event| this.process(event))
    }

    fn new<C, M>(client: &mut C, poster: P) -> anyhow::Result<Self>
    where
        C: mqtt::client::Client,
        M: mqtt::client::Message,
        C::Error: Debug + Display + Send + Sync + 'static,
    {
        client
            .subscribe("topic_todo", mqtt::client::QoS::AtMostOnce)
            .map_err(|e| anyhow::anyhow!(e))?;

        //impl Fn(&mqtt::client::Event<M>)

        let state = Self {
            poster,
            message_parser: Default::default(),
        };

        Ok(state)
    }

    fn process<M>(&mut self, mqtt_event: &mqtt::client::Event<M>) -> anyhow::Result<()>
    where
        M: mqtt::client::Message,
    {
        if let mqtt::client::Event::Received(ref message) = mqtt_event {
            if let Some(command) = self.message_parser.process(message) {
                self.poster
                    .post(Default::default(), &Self::EVENT_SOURCE, &command)
                    .map_err(|e| anyhow::anyhow!(e))?;
            }
        }

        Ok(())
    }
}

// pub struct MqttStatusUpdates<M, C, SV, SW> {
//     client: Rc<RefCell<M>>,
//     connection: C,
//     valve: Rc<RefCell<Valve<SV>>>,
//     water_meter: Rc<RefCell<WaterMeter<SW>>>,
// }

// impl<Q, C, SV, SW> MqttStatusUpdates<Q, C, SV, SW> {
//     pub const ID: &'static str = "MQTT_COMMANDS";

//     pub fn new<E>(
//         client: Rc<RefCell<Q>>,
//         connection: C,
//         valve: Rc<RefCell<Valve<SV>>>,
//         water_meter: Rc<RefCell<WaterMeter<SW>>>,
//     ) -> anyhow::Result<Rc<RefCell<Self>>>
//     where
//         Q: mqtt::client::Client<Error = E>,
//         C: mqtt::client::Connection,
//         E: Debug + Display + Send + Sync + 'static,
//     {
//         client
//             .borrow_mut()
//             .subscribe("topic_todo", mqtt::client::QoS::AtMostOnce)
//             .map_err(|e| anyhow::anyhow!(e))?;

//         let state = Self {
//             client,
//             connection,
//             valve,
//             water_meter,
//         };

//         Ok(Rc::new(RefCell::new(state)))
//     }
// }
