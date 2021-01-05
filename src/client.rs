use std::net::{SocketAddr, TcpStream};
use std::io::{self, Write, BufReader};
use std::fs::File;

use super::protocol::{self, ProtocolConnection};

type BoxResult<T> = Result<T, Box<dyn std::error::Error>>;

trait LoadFile {
    fn file_state(&mut self) -> &mut Option<File>;

    fn load_file<T: Into<String>>(&mut self, filepath: T) -> io::Result<()> {
        let file = File::open(filepath.into())?;
        *(self.file_state()) = Some(file);
        Ok(())
    }
}

impl LoadFile for Disconnected {
    fn file_state(&mut self) -> &mut Option<File> {
        &mut self.file
    }
}

impl LoadFile for Connected {
    fn file_state(&mut self) -> &mut Option<File> {
        &mut self.file
    }
}

impl<S> LoadFile for Client<S>
    where S: LoadFile
{
    fn file_state(&mut self) -> &mut Option<File> {
        self.state.file_state()
    }
}

impl ProtocolConnection for Connected {
    fn connection(&mut self) -> &mut TcpStream {
        &mut self.connection
    }
}

impl ProtocolConnection for Negotiating {
    fn connection(&mut self) -> &mut TcpStream {
        &mut self.connection
    }
}

impl ProtocolConnection for Sending {
    fn connection(&mut self) -> &mut TcpStream {
        &mut self.connection
    }
}

impl<S> ProtocolConnection for Client<S>
    where S: ProtocolConnection
{
    fn connection(&mut self) -> &mut TcpStream {
        self.state.connection()
    }
}
/// The client is used to send files to the server
#[derive(Debug)]
pub struct Client<S> {
    state: S,
    pub error: Option<Box<dyn std::error::Error>>,
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
    pub fn request<T: Into<String>>(mut self, filename: T) -> BoxResult<Client<Negotiating>> {
        if self.state.file.is_some() {
            self.send_message(protocol::Message::FileTransferRequest)?;
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
            if let Err(e) = self.send_message(protocol::Message::Goodbye) {
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
}

#[derive(Debug)]
pub struct Negotiating { connection: TcpStream, file: File }

impl Client<Negotiating> {
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
}

#[derive(Debug)]
pub struct Sending { connection: TcpStream, file: File }

impl Client<Sending> {
    pub fn finish(mut self) -> Result<Client<Connected>, Client<Sending>> {
        match self.send_message(protocol::Message::Goodbye) {
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

    pub fn send_file(&mut self) -> BoxResult<()> {
        let size = self.state.file.metadata()?.len();
        // send file size so server knows how much to read
        
        self.state.connection.write(&size.to_be_bytes())?;
        let mut buffer = BufReader::new(&mut self.state.file);
        io::copy(&mut buffer, &mut self.state.connection)?;
        Ok(())
    }
}
