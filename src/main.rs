use argh::FromArgs;

use fshare::{Client, Disconnected, ServerBuilder};

/// send or receive files between hosts
#[derive(FromArgs, PartialEq, Debug)]
struct Args {
    #[argh(subcommand)]
    subcommand: SubCommand,
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand)]
enum SubCommand {
    Client(ClientArgs),
    Server(ServerArgs),
}

/// Run the client to send files to an fshare server
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "client")]
struct ClientArgs {
    /// the address of the remote fshare server to send files to
    #[argh(option, short = 'a')]
    address: String,

    /// a relative or absolute path to the file to send
    #[argh(positional)]
    file: String,
}

/// Run the server to receive files from an fshare client
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "server")]
struct ServerArgs {
    /// the address to bind the server to
    #[argh(option, short = 'a', default = r#"String::from("0.0.0.0:8080")"#)]
    address: String,

    /// the directory in which to store received files
    #[argh(positional, default = r#"String::from("./")"#)]
    directory: String,
}

fn main() -> anyhow::Result<()> {
    let args: Args = argh::from_env();
    match args.subcommand {
        SubCommand::Client(args) => client(args.address, args.file),
        SubCommand::Server(args) => server(args.address, args.directory),
    }
}

fn client(address: String, file: String) -> anyhow::Result<()> {
    Client::<Disconnected>::new().send(address, file)
}

fn server(address: String, directory: String) -> anyhow::Result<()> {
    let mut server = ServerBuilder::new();
    server.directory(directory)?;
    let mut server = server.build()?;
    server.run(address)
}
