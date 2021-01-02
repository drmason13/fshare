/// fshare or "file share" is a peer-peer application to share files between computers over a network
///
/// Basic workflow:
/// 1. start the application on machine A to send files - choose a file and a Socket Address ip:port to send to
/// 2. start the application on machine B to receive files - choose a port to listen to connections on (or default) and a directory to write to
/// 3. application on A waits (polls) for a response from application on B
/// 4. application on A sends filename of the file to be transferred to B
/// 5. B acknowledges and accepts (or alters) filename
/// 6. A Streams file to B using TcpStream
/// 7. application on B Streams file from TcpListener to a file with the selected name in its Directory

mod server;
mod client;
mod protocol;

pub use server::ServerBuilder;
pub use client::{Client, Disconnected};

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
