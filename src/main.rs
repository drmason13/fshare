use fshare::{ServerBuilder, Client, Disconnected};

type BoxResult<T> = Result<T, Box<dyn std::error::Error>>;

#[derive(Debug)]
enum AppError {
    ArgsError(String)
}

impl std::error::Error for AppError {}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            AppError::ArgsError(s) => write!(f, "{}", s)
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("server") => server(args.next(), args.next()),
        Some("client") => client(args.next(), args.next()),
        _ => Err(Box::new(AppError::ArgsError(String::from("Incorrect Argument. use `fshare server` or `fshare client` to start the server or client respectively")))),
    }
}

fn client(address: Option<String>, file: Option<String>) -> BoxResult<()> {
    let file = file.ok_or(Box::new(AppError::ArgsError(String::from("Missing argument for file to load. Please provide a filepath to send to the server.\n\nfshare client <address> <filepath>"))))?;
    let address =  address.ok_or(Box::new(AppError::ArgsError("Missing address argument. Please provide an address to connect to the server.\n\nfshare client <address> <filepath>".to_string())))?;
    Client::<Disconnected>::new().send(address, file)
}

fn server(address: Option<String>, directory: Option<String>) -> BoxResult<()> {
    let mut builder = ServerBuilder::new();
    builder.directory(directory.unwrap_or("./".to_string()))?;
    let mut server = builder.build()?;
    server.run(address.unwrap_or("127.0.0.1:8080".to_string()))
}
