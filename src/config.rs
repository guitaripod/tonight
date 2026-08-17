use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub addr: SocketAddr,
    pub data_dir: PathBuf,
    pub steam_root: Option<PathBuf>,
    pub open_browser: bool,
}

impl Config {
    pub fn from_env_and_args() -> anyhow::Result<Self> {
        let mut addr_s = std::env::var("TONIGHT_BIND").unwrap_or_else(|_| "0.0.0.0:7777".into());
        let mut data_dir = std::env::var_os("TONIGHT_DATA_DIR").map(PathBuf::from);
        let mut steam_root = std::env::var_os("TONIGHT_STEAM_ROOT").map(PathBuf::from);
        let mut open_browser = false;

        let mut args = std::env::args().skip(1);
        while let Some(a) = args.next() {
            match a.as_str() {
                "--bind" | "-b" => addr_s = args.next().unwrap_or(addr_s),
                "--port" | "-p" => {
                    if let Some(p) = args.next() {
                        addr_s = format!("0.0.0.0:{p}");
                    }
                }
                "--data" | "-d" => data_dir = args.next().map(PathBuf::from).or(data_dir),
                "--steam" => steam_root = args.next().map(PathBuf::from).or(steam_root),
                "--open" => open_browser = true,
                "--version" | "-V" => {
                    println!("tonight {}", env!("CARGO_PKG_VERSION"));
                    std::process::exit(0);
                }
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                other => anyhow::bail!("unknown argument: {other} (try --help)"),
            }
        }

        let data_dir = data_dir.unwrap_or_else(default_data_dir);
        std::fs::create_dir_all(&data_dir)?;
        std::fs::create_dir_all(data_dir.join("img"))?;

        Ok(Config {
            addr: addr_s.parse()?,
            data_dir,
            steam_root: steam_root.filter(|p| p.exists()).or_else(detect_steam_root),
            open_browser,
        })
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("tonight.db")
    }

    pub fn img_dir(&self) -> PathBuf {
        self.data_dir.join("img")
    }
}

fn print_help() {
    println!(
        "tonight {v} — a decision engine for your Steam library

USAGE:
    tonight [OPTIONS]

OPTIONS:
    -b, --bind <ADDR>   Listen address (default 0.0.0.0:7777, env TONIGHT_BIND)
    -p, --port <PORT>   Shorthand for --bind 0.0.0.0:<PORT>
    -d, --data <DIR>    Data directory (env TONIGHT_DATA_DIR)
        --steam <DIR>   Steam root, e.g. ~/.steam/steam (env TONIGHT_STEAM_ROOT)
        --open          Open a browser once the server is up
    -V, --version       Print version
    -h, --help          Print this help

Everything else — Steam API key, launching, timers — is configured in the web UI.
No key is required: tonight reads your local Steam client files by default.",
        v = env!("CARGO_PKG_VERSION")
    );
}

fn default_data_dir() -> PathBuf {
    if let Some(d) = dirs::data_dir() {
        return d.join("tonight");
    }
    PathBuf::from("./data")
}

/// Locates a Steam client installation across native, Flatpak, Snap, macOS and Windows layouts.
pub fn detect_steam_root() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let candidates = [
        home.join(".steam/steam"),
        home.join(".local/share/Steam"),
        home.join(".steam/root"),
        home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
        home.join("snap/steam/common/.local/share/Steam"),
        home.join("Library/Application Support/Steam"),
        PathBuf::from("C:\\Program Files (x86)\\Steam"),
    ];
    candidates
        .into_iter()
        .find(|p| p.join("steamapps").is_dir() || p.join("userdata").is_dir())
}
