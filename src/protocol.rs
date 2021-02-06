/// Protocol
///
/// ```text
///   Client     |                             | Server
///  ------------|                             |------------------
/// Disconnected |                             | Listening
///              |-- <Successful Connection> ->|
///    Connected |                             | Connected
///              |---- FileTransferRequest --->|
///    Connected |                             | Connected
///              |<---------- Ack -------------|
///  Negotiating |                             | Negotiating
///              |----- <Stream FileName> ---->|
///  Negotiating |                             | Negotiating
///              |<---------- Ack -------------|
///      Sending |                             | Receiving
///              |--- <Stream File Content> -->|
///      Sending |                             | Receiving
///              |<---------- Ack -------------|
///    Connected |                             | Connected
///              |--------- Goodbye ---------->|
///    Connected |                             | Connected
///              |<-------- Goodbye -----------|
/// Disconnected |                             | Listening
/// ```
use std::convert::TryFrom;
use std::io::{Read, Write};
use std::net::TcpStream;

use anyhow::bail;

/// "Phases" of the protocol, or states for the server to track progress of each connection
/// The server will match on this to decide how to read incoming data and interpret messages
#[derive(Debug)]
pub enum State {
    Connected,
    Negotiating,
    Receiving,
}

/// Both Client and Server while connected can send and receive protocol messages
pub(crate) trait ProtocolConnection {
    /// A mutable reference to your connection, used to send and receive protocol messages
    fn connection(&mut self) -> &mut TcpStream;

    /// Send a protocol message through the connection
    fn send_message(&mut self, message: Message) -> anyhow::Result<()> {
        self.connection().write(&message.as_bytes())?;
        Ok(())
    }

    /// Receive a protocol message from the connection
    fn receive_message(&mut self) -> anyhow::Result<Message> {
        let mut buffer = [0; 1];
        self.connection().read(&mut buffer)?;
        let message = Message::try_from(buffer[0])?;
        Ok(message)
    }
}

trait Client {
    fn send_filename<T: Into<String>>(&mut self, filename: T) -> anyhow::Result<()>;
}

/// Messages passed between Client and Server
#[derive(Debug)]
pub enum Message {
    FileTransferRequest,
    RequestDenied,
    Ack,
    Goodbye,
}

impl TryFrom<u8> for Message {
    type Error = anyhow::Error;

    fn try_from(byte: u8) -> Result<Self, Self::Error> {
        match byte {
            30 => Ok(Message::FileTransferRequest),
            43 => Ok(Message::RequestDenied),
            200 => Ok(Message::Ack),
            255 => Ok(Message::Goodbye),
            _ => bail!("Could not decode message: `{}`", byte),
        }
    }
}

impl Message {
    pub fn as_bytes(self) -> [u8; 1] {
        match self {
            Message::FileTransferRequest => [30],
            Message::RequestDenied => [43],
            Message::Ack => [200],
            Message::Goodbye => [255],
        }
    }
}
