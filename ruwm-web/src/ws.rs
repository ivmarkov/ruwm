use futures::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};

use gloo_net::websocket::{futures::WebSocket, Message};

use ruwm::web_dto::{WebEvent, WebRequest};

use postcard::*;

use crate::error;

pub fn open(url: &str) -> error::Result<(WebSender, WebReceiver)> {
    let ws = WebSocket::open(url).map_err(|e| anyhow::anyhow!("{}", e))?;

    let (write, read) = ws.split();

    Ok((WebSender(write), WebReceiver(read)))
}

pub struct WebSender(SplitSink<WebSocket, Message>);

impl WebSender {
    pub async fn send(&mut self, request: &WebRequest) -> error::Result<()> {
        self.0
            .send(Message::Bytes(to_allocvec(request)?))
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        Ok(())
    }
}

pub struct WebReceiver(SplitStream<WebSocket>);

impl WebReceiver {
    pub async fn recv(&mut self) -> error::Result<WebEvent> {
        let message = self
            .0
            .next()
            .await
            .unwrap()
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        let event = match message {
            Message::Bytes(data) => from_bytes(&data)?,
            _ => anyhow::bail!("Invalid message format"),
        };

        Ok(event)
    }
}
