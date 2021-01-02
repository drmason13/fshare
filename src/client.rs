use std::net::{SocketAddr, TcpStream};
use std::io::{self, Read, Write, BufReader};
use std::fs::File;
use std::convert::TryFrom;

use super::protocol;

type BoxResult<T> = Result<T, Box<dyn std::error::Error>>;

/// The client is used to send files to the server
#[derive(Debug)]
pub struct Client<S> {
    state: S,
    pub error: Option<Box<dyn std::error::Error>>,
}

fn load_file<T: Into<String>>(filepath: T) -> io::Result<File> {
    let file = File::open(filepath.into())?;
    Ok(file)
}

impl Client<Disconnected> {
    pub fn new() -> Client<Disconnected> {
        Client {
            state: Disconnected { file: None },
            error: None,
        }
    }
}

#[derive(Debug)]
pub struct Disconnected { file: Option<File> }

impl Client<Disconnected> {
    pub fn try_connection<S: Into<String>>(&self, connection_string: S) -> BoxResult<TcpStream> {
        let remote_addr = connection_string.into().parse::<SocketAddr>()?;
        Ok(TcpStream::connect(remote_addr)?)
    }

    pub fn connect<S: Into<String>>(self, connection_string: S) -> Result<Client<Connected>, Client<Disconnected>> {
        match self.try_connection(connection_string) {
            Ok(connection) => {
               connection.set_read_timeout(Some(std::time::Duration::new(5, 0))).unwrap();
               Ok(Client { state: Connected { connection, file: self.state.file }, error: None })
            },
            Err(error) => Err(Client { state: Disconnected { file: self.state.file }, error: Some(error) }),
        }
    }

    pub fn load_file<T: Into<String>>(&mut self, filepath: T) -> io::Result<()> {
        self.state.file = Some(load_file(filepath)?);
        Ok(())
    }

    /// Convenience method for end user to send a file using the configured client
    pub fn send(mut self, address: String, file: String) -> BoxResult<()> {
        self.load_file(&file)?;
        /* Convenient API to aim for...
        client
            .file(file)?
            .connect(address)?
            .negotiate()?
            .send()?
        Or something to that effect - chained method calls :)
        */

        match self.connect(address) {
            Ok(connected_client) => {
                let mut negotiating_client = connected_client.request(&file)?;
                if let Ok(protocol::Message::Ack) = negotiating_client.receive_message() {
                    let mut sending_client = negotiating_client.accept();
                    sending_client.send_file()?;
                    if let Ok(protocol::Message::Ack) = sending_client.receive_message() {
                        println!("Server acknowledged receipt of file");
                    }
                    println!("Closing connection");
                    if let Ok(connected_client) = sending_client.finish() {
                        let _disconnected_client = connected_client.goodbye();
                    }
                } else {
                    let connected_client = negotiating_client.deny();
                    let _disconnected_client = connected_client.goodbye();
                    println!("Disconnected, the server did not accept our request");
                }
            },
            Err(e) => {
                eprintln!("Unable to connect: {}", e.error.unwrap());
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct Connected { connection: TcpStream, file: Option<File> }

impl Client<Connected> {
    fn message(&mut self, message: protocol::Message) -> BoxResult<()> {
        self.state.connection.write(&message.as_bytes())?;
        Ok(())
    }

    pub fn receive_message(&mut self) -> BoxResult<protocol::Message> {
        // TODO: Interpret response from server and act accordingly
        // Currently we're persisting with the awkward state machine approach for the client
        // so the application code will need to call accept or deny for us based on the message :(
        let mut buffer = [0; 1];
        self.state.connection.read(&mut buffer)?;
        let message = protocol::Message::try_from(buffer[0])?;
        println!("received message from server: {:?}", &message);
        Ok(message)
    }

    pub fn request<T: Into<String>>(mut self, filename: T) -> BoxResult<Client<Negotiating>> {
        if self.state.file.is_some() {
            self.message(protocol::Message::FileTransferRequest)?;
            if let protocol::Message::Ack = self.receive_message()? {
                self.send_filename(filename)?;
                Ok(Client {
                    state: Negotiating { connection: self.state.connection, file: self.state.file.unwrap(), },
                    error: None,
                })
            } else {
                Err(Box::new(protocol::Error::Message))
            }
        } else {
            Err(Box::new(protocol::Error::Behaviour))
        }
    }

    pub fn send_filename<T: Into<String>>(&mut self, filename: T) -> io::Result<()> {
        let filename = filename.into();
        self.state.connection.write_all(filename.as_bytes())?;
        println!("sent filename: {}", filename);
        Ok(())
    }

    fn disconnect(self, error: Option<Box<dyn std::error::Error>>) -> Client<Disconnected> {
        Client {
            state: Disconnected { file: self.state.file },
            error,
        }
    }

    pub fn goodbye(mut self) -> Client<Disconnected> {
        let max_attempts = 5;
        let mut attempt = 0;
        // Say Goodbye and wait for a Goodbye from server (or timeout)
        loop {
            if let Err(e) = self.message(protocol::Message::Goodbye) {
                eprintln!("Error saying Goodbye: Attempt {}", attempt);
                attempt += 1;
                if attempt >= max_attempts {
                    eprintln!("Max attempts to say Goodbye reached. Disconnecting");
                    break self.disconnect(Some(e))
                };
            } else {
                if let Ok(protocol::Message::Goodbye) = self.receive_message() {
                    // close connection without error
                    break self.disconnect(None)
                }
            }
        }
    }

    pub fn load_file<T: Into<String>>(&mut self, filepath: T) -> io::Result<()> {
        self.state.file = Some(load_file(filepath)?);
        Ok(())
    }
}

#[derive(Debug)]
pub struct Negotiating { connection: TcpStream, file: File }

impl Client<Negotiating> {
    pub fn receive_message(&mut self) -> BoxResult<protocol::Message> {
        // TODO: Interpret response from server and act accordingly
        // Currently we're persisting with the awkward state machine approach for the client
        // so the application code will need to call accept or deny for us based on the message :(
        let mut buffer = [0; 1];
        self.state.connection.read(&mut buffer)?;
        let message = protocol::Message::try_from(buffer[0])?;
        println!("received message from server: {:?}", &message);
        Ok(message)
    }

    pub fn accept(self) -> Client<Sending> {
        Client {
            state: Sending { connection: self.state.connection, file: self.state.file },
            error: None,
        }
    }

    pub fn deny(self) -> Client<Connected> {
        Client {
            state: Connected { connection: self.state.connection, file: None },
            error: None,
        }
    }

    pub fn load_file<T: Into<String>>(&mut self, filepath: T) -> io::Result<()> {
        self.state.file = load_file(filepath)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct Sending { connection: TcpStream, file: File }

impl Client<Sending> {
    pub fn finish(mut self) -> Result<Client<Connected>, Client<Sending>> {
        match self.message(protocol::Message::Goodbye) {
            Ok(_) => Ok(Client {
                state: Connected { connection: self.state.connection, file: None },
                error: None,
            }),
            Err(e) => Err(Client {
                state: Sending { ..self.state },
                error: Some(e),
            }),
        }
    }


    pub fn send_file(&mut self) -> io::Result<()> {
        let size = self.state.file.metadata()?.len();
        let mut buffer = BufReader::new(&mut self.state.file);
        // send file size so server knows how much to read
        self.state.connection.write(&size.to_be_bytes())?;
        io::copy(&mut buffer, &mut self.state.connection)?;
        Ok(())
    }

    fn message(&mut self, message: protocol::Message) -> BoxResult<()> {
        self.state.connection.write(&message.as_bytes())?;
        Ok(())
    }

    pub fn receive_message(&mut self) -> BoxResult<protocol::Message> {
        // TODO: Interpret response from server and act accordingly
        // Currently we're persisting with the awkward state machine approach for the client
        // so the application code will need to call accept or deny for us based on the message :(
        let mut buffer = [0; 1];
        self.state.connection.read(&mut buffer)?;
        let message = protocol::Message::try_from(buffer[0])?;
        println!("received message from server: {:?}", &message);
        Ok(message)
    }
}
