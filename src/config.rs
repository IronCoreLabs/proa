use std::ffi::OsString;

use clap::Parser;
use reqwest::Url;

/// Command line arguments.
#[derive(Parser)]
#[command(name = "proa", author, version, about)]
pub struct Cli {
    /// URLs to GET, to prompt containers to shut down
    #[arg(short = 'g', long)]
    pub shutdown_http_get: Vec<Url>,
    /// URLs to POST to, to prompt containers to shut down
    #[arg(short = 'p', long)]
    pub shutdown_http_post: Vec<Url>,

    /// Process names to send SIGTERM to on shutdown
    #[cfg(feature = "kill")]
    #[arg(short, long, id = "PROCNAME")]
    pub kill: Vec<OsString>,
    /// Send SIGTERM to all other visible processes on shutdown
    #[cfg(feature = "kill")]
    #[arg(short = 'K', long)]
    pub kill_all: bool,

    /// The command to run once sidecars are ready
    pub command: OsString,
    /// Arguments to pass to the command
    pub args: Vec<OsString>,
}
