//! fshare or "file share" is a peer-peer application to share files between computers over a network
//!
//! > This is a toy-project written to practice writing rust networking applications
//!
//! Having said that, it may be a useful reference to get started with `std::io` networking primitives such as `Read`, `Write` and `TcpStream`.
//!
//! There are no network related dependencies. Originally there were 0 dependencies but at the moment it depends on a few crates for convenience:
//!
//! * **anyhow** - simple error handling ideal for applications
//! * **argh** - opinionated command line parsing
//!
//! It is a functional tool for sending and receiving files on the network though its features are limited in scope.
//!
//! # Usage
//! ```text
//! Usage: fshare server [<directory>] [-a <address>]
//!
//! Run the server to receive files from an fshare client
//!
//! Options:
//!   -a, --address     the address to bind the server to
//!   --help            display usage information
//! ```
//!
//! ```text
//! Usage: fshare client <file> -a <address>
//!
//! Run the client to send files to an fshare server
//!
//! Options:
//!   -a, --address     the address of the remote fshare server to send files to
//!   --help            display usage information
//! ```
//!
//! # Basic workflow:
//! To send a file from A to B using fshare
//! 1. start the server on machine B to receive files - choose a port to listen to connections on and a directory to write to
//! 1. start the client on machine A to send files - choose a file and a Socket Address ip:port to send to
//! 1. client connects to server
//! 1. client sends filename of the file to be transferred to server
//! 1. server acknowledges and accepts (or alters) filename
//! 1. client Streams file to server using TcpStream
//! 1. server Streams file from TcpListener to a file with the selected name in the directory
//!
//! # Internals
//! * A shared protocol is used between client and server, as specified in [fshare::protocol]
//! * both [fshare:client] and [fshare::server] implement the trait [protocol::ProtocolConnection] to send messages to each other
//! * [protocol::Client] experiments with a state machine approach to enforce proper usage at compile-time
//!     * Each stage of the protocol maps to a specific type of Client, e.g. a `Client<Negotiating>` is in the middle of negotiating the filename of the file to transfer
//! * [protocol::Server] takes a different, more flexible approach, using the [protocol::State] enum to match on and do control flow
//!     * It will mutate itself rather than force you to return a new type.
//! * The difficulty of using the client's state machine approach led me to write a helper function [client::send] to make using it to send a file much simpler!
mod client;
mod protocol;
mod server;

pub use client::{Client, Disconnected};
pub use server::ServerBuilder;
