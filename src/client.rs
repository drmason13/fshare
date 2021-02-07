use std::fs::File;
use std::io::{self, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;

use super::protocol::{self, ProtocolConnection};

use anyhow::{anyhow, bail, Context};

trait LoadFile {
    fn file_state(&mut self) -> &mut Option<File>;
    fn filename_state(&mut self) -> &mut Option<String>;

    fn load_file<T: Into<String>>(&mut self, filepath: T) -> anyhow::Result<()> {
        // grab the file_name part of filepath
        // first parse into a PathBuf
        let filepath = &filepath.into();

        let path_buf = &filepath.parse::<PathBuf>().with_context(|| format!("Could not load file: `{}`, is it a directory?\nYou can only send one file at a time", &filepath))?;
        // then convert to a utf8 string, which is lossy due to differences in how windows and linux store strings, but infallible
        // the ok_or is because ".." is a valid PathBuf but its file_name() is None
        let name = path_buf.file_name().ok_or(anyhow!("Could not load file: `{}`, is it a directory?\nYou can only send one file at a time", &filepath))?.to_string_lossy().to_string();
        // we store the name in state to send to the server later
        *(self.filename_state()) = Some(name);
        // finally we can actually open the file
        let file = File::open(path_buf).with_context(|| format!("Failed to read file: `{}`, is it a directory?\nYou can only send one file at a time", &filepath))?;
        *(self.file_state()) = Some(file);
        dbg!(self.filename_state());
        Ok(())
    }
}

impl LoadFile for Disconnected {
    fn file_state(&mut self) -> &mut Option<File> {
        &mut self.file
    }

    fn filename_state(&mut self) -> &mut Option<String> {
        &mut self.filename
    }
}

impl LoadFile for Connected {
    fn file_state(&mut self) -> &mut Option<File> {
        &mut self.file
    }

    fn filename_state(&mut self) -> &mut Option<String> {
        &mut self.filename
    }
}

impl<S> LoadFile for Client<S>
where
    S: LoadFile,
{
    fn file_state(&mut self) -> &mut Option<File> {
        self.state.file_state()
    }

    fn filename_state(&mut self) -> &mut Option<String> {
        self.state.filename_state()
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
where
    S: ProtocolConnection,
{
    fn connection(&mut self) -> &mut TcpStream {
        self.state.connection()
    }
}
/// The client is used to send files to the server
#[derive(Debug)]
pub struct Client<S> {
    state: S,
    pub error: Option<anyhow::Error>,
}

#[derive(Debug)]
pub struct Disconnected {
    file: Option<File>,
    filename: Option<String>,
}

impl Client<Disconnected> {
    pub fn new() -> Client<Disconnected> {
        Client {
            state: Disconnected {
                file: None,
                filename: None,
            },
            error: None,
        }
    }

    pub fn try_connection<S: Into<String>>(
        &self,
        connection_string: S,
    ) -> anyhow::Result<TcpStream> {
        let remote_addr = connection_string.into().parse::<SocketAddr>()?;
        Ok(TcpStream::connect(remote_addr)?)
    }

    pub fn connect<S: Into<String>>(
        self,
        connection_string: S,
    ) -> Result<Client<Connected>, Client<Disconnected>> {
        match self.try_connection(connection_string) {
            Ok(connection) => {
                connection
                    .set_read_timeout(Some(std::time::Duration::new(5, 0)))
                    .unwrap();
                Ok(Client {
                    state: Connected {
                        connection,
                        file: self.state.file,
                        filename: self.state.filename,
                    },
                    error: None,
                })
            }
            Err(error) => Err(Client {
                state: Disconnected {
                    file: self.state.file,
                    filename: self.state.filename,
                },
                error: Some(error),
            }),
        }
    }

    /// Convenience method for end user to send a file using the configured client
    pub fn send(mut self, address: String, file: String) -> anyhow::Result<()> {
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
                let mut negotiating_client = connected_client.request()?;
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
            }
            Err(e) => {
                eprintln!("Unable to connect: {}", e.error.unwrap());
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct Connected {
    connection: TcpStream,
    file: Option<File>,
    filename: Option<String>,
}

impl Client<Connected> {
    pub fn request(mut self) -> anyhow::Result<Client<Negotiating>> {
        if self.state.file.is_some() {
            if self.state.filename.is_some() {
                self.send_message(protocol::Message::FileTransferRequest)?;
                let received = self.receive_message()?;
                if let protocol::Message::Ack = received {
                    self.send_filename()?;
                    Ok(Client {
                        state: Negotiating {
                            connection: self.state.connection,
                            file: self.state.file.unwrap(),
                            filename: self.state.filename.unwrap(),
                        },
                        error: None,
                    })
                } else {
                    bail!("Expected Ack, received: `{:?}`", received)
                }
            } else {
                bail!("Cannot request to transfer file: no filename has been configured!")
            }
        } else {
            bail!("Cannot request to transfer file: no file has been configured!")
        }
    }

    pub fn send_filename(&mut self) -> anyhow::Result<()> {
        let filename = self.state.filename.clone().ok_or(anyhow!(
            "Could not send_filename because it has not been configured"
        ))?;

        self.connection().write_all(filename.clone().as_bytes())?;
        println!("sent filename: {}", filename);
        Ok(())
    }

    fn disconnect(self, error: Option<anyhow::Error>) -> Client<Disconnected> {
        Client {
            state: Disconnected {
                file: self.state.file,
                filename: self.state.filename,
            },
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
                    break self.disconnect(Some(e));
                };
            } else {
                if let Ok(protocol::Message::Goodbye) = self.receive_message() {
                    // close connection without error
                    break self.disconnect(None);
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct Negotiating {
    connection: TcpStream,
    file: File,
    filename: String,
}

impl Client<Negotiating> {
    pub fn accept(self) -> Client<Sending> {
        Client {
            state: Sending {
                connection: self.state.connection,
                file: self.state.file,
            },
            error: None,
        }
    }

    pub fn deny(self) -> Client<Connected> {
        Client {
            state: Connected {
                connection: self.state.connection,
                file: None,
                filename: None,
            },
            error: None,
        }
    }
}

#[derive(Debug)]
pub struct Sending {
    connection: TcpStream,
    file: File,
}

impl Client<Sending> {
    pub fn finish(mut self) -> Result<Client<Connected>, Client<Sending>> {
        match self.send_message(protocol::Message::Goodbye) {
            Ok(_) => Ok(Client {
                state: Connected {
                    connection: self.state.connection,
                    file: None,
                    filename: None,
                },
                error: None,
            }),
            Err(e) => Err(Client {
                state: Sending { ..self.state },
                error: Some(e),
            }),
        }
    }

    pub fn send_file(&mut self) -> anyhow::Result<()> {
        let size = self.state.file.metadata()?.len();
        // send file size so server knows how much to read
        // TODO security - we should send the file size sooner so that it can be negotiated, but then confirm the file size is the same (perhaps it was written to in the meantime by another process?)
        self.state.connection.write(&size.to_be_bytes())?;

        let mut buffer = BufReader::new(&mut self.state.file);
        io::copy(&mut buffer, &mut self.state.connection)?;
        Ok(())
    }
}
