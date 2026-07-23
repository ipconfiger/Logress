use clap::{Parser, ValueEnum};
use std::path::PathBuf;

/// Real-time Loki log tailing via Grafana Data Source Proxy
#[derive(Parser, Debug)]
#[command(name = "graftail", version, about, long_about = None)]
pub struct Cli {
    /// Grafana base URL (e.g. https://grafana.example.com)
    #[arg(long, env = "GRAFTAIL_URL", global = true)]
    pub grafana_url: Option<String>,

    /// Loki datasource UID in Grafana
    #[arg(long, env = "GRAFTAIL_DATASOURCE_UID", global = true)]
    pub datasource_uid: Option<String>,

    /// LogQL query string (required for tail mode)
    #[arg(short = 'q', long, env = "GRAFTAIL_QUERY")]
    pub query: Option<String>,

    /// Grafana API Token / Service Account Token
    #[arg(long, env = "GRAFTAIL_TOKEN", global = true)]
    pub token: Option<String>,

    /// Grafana username (for Basic Auth)
    #[arg(long, env = "GRAFTAIL_USER", global = true)]
    pub user: Option<String>,

    /// Grafana password (for Basic Auth) — prefer env var or interactive prompt
    #[arg(long, env = "GRAFTAIL_PASSWORD", hide_env_values = true, global = true)]
    pub password: Option<String>,

    /// Number of historical log lines to fetch before tailing
    #[arg(long, default_value_t = 0)]
    pub last: usize,

    /// Start time for tail (e.g. "1h", "30m")
    #[arg(long)]
    pub since: Option<String>,

    /// Output format
    #[arg(long, default_value_t = OutputFormat::Pretty)]
    pub output: OutputFormat,

    /// Path to config file
    #[arg(long, default_value = "~/.config/graftail/config.toml", global = true)]
    pub config: PathBuf,

    /// Use UTC timestamps instead of local time
    #[arg(long)]
    pub utc: bool,

    /// Skip TLS certificate verification (for self-signed/internal CA certificates)
    #[arg(long, global = true)]
    pub insecure: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// List available labels and their values from Loki
    List {
        /// Show values for a specific label (default: show all labels and their values)
        #[arg(short, long)]
        label: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    /// Colored terminal output (default)
    Pretty,
    /// Machine-readable JSON lines
    Json,
    /// Plain text, no ANSI color codes (suitable for piping)
    Plain,
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputFormat::Pretty => write!(f, "pretty"),
            OutputFormat::Json => write!(f, "json"),
            OutputFormat::Plain => write!(f, "plain"),
        }
    }
}
