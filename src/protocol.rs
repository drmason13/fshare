/// Protocol
///
/// ```
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

/// "Phases" of the protocol, or states for the server to track progress of each connection
/// The server will match on this to decide how to read incoming data and interpret messages
#[derive(Debug)]
pub enum State {
    Connected,
    Negotiating,
    Receiving,
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
    type Error = Error;

    fn try_from(byte: u8) -> Result<Self, Self::Error> {
        match byte {
            30 => Ok(Message::FileTransferRequest),
            43 => Ok(Message::RequestDenied),
            200 => Ok(Message::Ack),
            255 => Ok(Message::Goodbye),
            _ => Err(Error::Message),
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

#[derive(Debug)]
pub enum Error {
    Generic(String),
    Message,
    Behaviour,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::Generic(error) => write!(f, "Error: {}", error),
            Error::Message => write!(f, "Protocol Message Error. The message could not be encoded/decoded properly due to invalid input"),
            Error::Behaviour => write!(f, "Protocol Behaviour Error. The protocol is not being followed correctly, stop it you!"),
        }
    }
}

impl std::error::Error for Error {}
