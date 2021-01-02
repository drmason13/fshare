use std::net::{ToSocketAddrs, TcpStream, TcpListener};
use std::io::{Read, Write, BufWriter, BufReader, BufRead};
use std::path::PathBuf;
use std::fs::{self, File};
use std::convert::TryFrom;

use super::protocol;

type BoxResult<T> = Result<T, Box<dyn std::error::Error>>;

/// The server needs to know what port to listen to and what directory to save incoming files to
/// The server maintains the TcpStream and communicates with the client to acknowledge incoming files
pub struct ServerBuilder {
    directory: Option<PathBuf>,
}

#[derive(Debug)]
pub struct Server {
    connection: Option<TcpStream>,
    directory: PathBuf,
    state: Option<protocol::State>,
    filename: Option<String>,
}

#[derive(Debug)]
pub enum ServerBuildError {
    NoDirectoryConfigured,
}

impl std::error::Error for ServerBuildError {}

impl std::fmt::Display for ServerBuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::NoDirectoryConfigured => write!(f, "Please configure a directory before listening"),
        }
    }
}

impl ServerBuilder {
    pub fn new() -> Self {
        ServerBuilder {
            directory: None,
        }
    }

    /// Configures a directory to save received files to
    pub fn directory<T: Into<PathBuf>>(&mut self, path: T) -> BoxResult<()> {
        let mut path = path.into();
        path.push("fshare_write_test");
        {
            let _f = File::create(&path)?;
        }
        fs::remove_file(&path)?;
        path.pop();
        self.directory = Some(path);
        Ok(())
    }

    /// Builds the Server and has it listen to a given address
    /// Returns a ServerBuildError if a directory hasn't previously been configured
    pub fn build(self) -> BoxResult<Server> {
        if self.directory.is_none() { return Err(Box::new(ServerBuildError::NoDirectoryConfigured)) }
        Ok(Server {
            connection: None,
            directory: self.directory.unwrap(),
            filename: None,
            state: None,
        })
    }
}

impl Server {
    pub fn run(&mut self, addr: impl ToSocketAddrs) -> BoxResult<()> {
        let listener = TcpListener::bind(addr)?;
        for stream in listener.incoming() {
            // set timeout
            let stream = stream?;
            stream.set_read_timeout(Some(std::time::Duration::new(5, 0))).unwrap();
            // Connect to the incoming stream
            self.connection = Some(stream);
            self.state = Some(protocol::State::Connected);
            self.progress_protocol()?;
            println!("Protocol Completed");
        }
        Ok(())
    }
}

impl Server {
    /// Recursively read data from the stream (self.connection) and act according to internal state
    /// The connection will close if/when we receive a Goodbye Message while in a Connected state
    fn progress_protocol(&mut self) -> BoxResult<()> {
        match self.state {
            Some(protocol::State::Connected) => {
                let message = self.receive_message()?;
                self.handle_message(message)
            },
            Some(protocol::State::Negotiating) => {
                self.receive_filename()?;
                self.send_message(protocol::Message::Ack)?;
                self.state = Some(protocol::State::Receiving);
                self.progress_protocol()
            },
            Some(protocol::State::Receiving) => {
                self.receive_file()?;
                self.send_message(protocol::Message::Ack)?;
                self.state = Some(protocol::State::Connected);
                self.progress_protocol()
            },
            None => {
                eprintln!("Server is in Invalid state:\n{:?}", &self);
                Err(Box::new(protocol::Error::Behaviour))
            },
        }
    }

    fn receive_message(&mut self) -> BoxResult<protocol::Message> {
        // All our protocol messages serialize to a single byte!
        let mut buf = [0; 1];
        self.connection.as_mut().unwrap().read(&mut buf)?;
        let message = protocol::Message::try_from(buf[0])?;
        println!("Received message from client: {:?}", &message);
        Ok(message)
    }

    fn receive_filename(&mut self) -> BoxResult<()> {
        // Currently we auto accept any filename
        let mut reader = BufReader::new(self.connection.as_mut().unwrap());
        let received: Vec<u8> = reader.fill_buf()?.to_vec();
        dbg!(&received);
        reader.consume(received.len());
        self.filename = Some(String::from_utf8(received)?);
        println!("filename received: {:?}", &self.filename);
        Ok(())
    }

    fn receive_file(&mut self) -> BoxResult<()> {
        // prepare reader (stream)
        let mut reader = BufReader::new(self.connection.as_mut().unwrap());

        // read file size
        let mut size = [0; 8];
        reader.read(&mut size)?;
        let mut size = u64::from_be_bytes(size);

        // prepare writer (file) so that we can start writing to the file
        let mut full_path = self.directory.clone();
        let temp_path = PathBuf::from(self.filename.as_ref().unwrap());
        let filename = temp_path.file_name().ok_or(Box::new(protocol::Error::Generic(String::from("Empty filename received!"))))?;
        full_path.push(filename);
        let file = File::create(full_path)?;
        let mut writer = BufWriter::new(file);

        // read until we have read all of the file according to the size received from client
        // TODO: Security sanity check on file size?
        while size > 0 {
            let received: Vec<u8> = reader.fill_buf()?.to_vec();
            writer.write_all(&received)?;
            size -= received.len() as u64;
            reader.consume(received.len());
        }
        writer.flush()?;
        Ok(())
    }

    fn handle_message(&mut self, message: protocol::Message) -> BoxResult<()> {
        match message {
            protocol::Message::Goodbye => {
                // This should finish the protocol and now we can continue listening for new connections
                self.goodbye()
            },
            protocol::Message::FileTransferRequest => {
                // Send Ack in reply
                self.send_message(protocol::Message::Ack)?;
                // change state to Negotiating
                self.state = Some(protocol::State::Negotiating);
                self.progress_protocol()
            },
            _ => {
                // Unexpected message, error and Goodbye (MVP)
                eprintln!("UNEXPECTED MESSAGE RECEIVED GOODBYE!");
                self.goodbye()
            }
        }
    }

    fn goodbye(&mut self) -> BoxResult<()> {
        // Send a Goodbye in reply
        // close the connection and reset state
        // this function must not be called if connection is not yet initialised
        let max_attempts = 10;
        let attempt = 0;
        loop {
            if let Err(e) = self.send_message(protocol::Message::Goodbye) {
                eprintln!("Error saying Goodbye: Attempt {}", attempt);
                if attempt < max_attempts {
                    eprintln!("Max attempts to say Goodbye reached");
                    break (Err(e))
                }
            } else {
                self.connection.as_mut().unwrap().shutdown(std::net::Shutdown::Read)?;
                self.connection = None;
                self.state = None;
                break Ok(())
            }
        }
    }

    fn send_message(&mut self, message: protocol::Message) -> BoxResult<()> {
        println!("{:?}", &message);
        let bytes = message.as_bytes();
        // this function must not be called if connection is not yet initialised
        self.connection.as_mut().unwrap().write(&bytes)?;
        Ok(())
    }
}
