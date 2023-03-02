use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct Opt {
    /// Bind the TCP server to this host.
    #[structopt(short, long, default_value = "127.0.0.1")]
    pub host: String,

    /// Bind the TCP server to this port.
    #[structopt(short, long, default_value = "26358")]
    pub port: u16,

    /// Bind the gRPC server to this port.
    #[structopt(short, long, default_value = "9093")]
    pub grpc_port: u16,

    /// The default server read buffer size, in bytes, for each incoming client
    /// connection.
    #[structopt(short, long, default_value = "1048576")]
    pub read_buf_size: usize,

    /// Increase output logging verbosity to DEBUG level.
    #[structopt(short, long)]
    pub verbose: bool,

    /// Suppress all output logging (overrides --verbose).
    #[structopt(short, long)]
    pub quiet: bool,
}
