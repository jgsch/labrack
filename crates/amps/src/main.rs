//! `labgruppen`: command line for testing the library against real amps (or the IPD simulator).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
use labgruppen::{Amp, Driver, Meters, Param, Snapshot, Value};

#[derive(Parser)]
#[command(name = "labgruppen", version, about = "Control Lab.gruppen IPX / IPD amplifiers")]
struct Cli {
    /// Amp address (or $LABGRUPPEN_HOST)
    #[arg(long, env = "LABGRUPPEN_HOST", global = true)]
    host: Option<String>,
    /// Amp family; detected from the open control port when omitted
    #[arg(long, value_parser = ["ipx", "ipd"], global = true)]
    driver: Option<String>,
    /// More logging (-v info, -vv debug)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Find amps: IPX broadcasts, IPD announcements, optionally scan a /24 for IPD and IPX units
    Discover {
        #[arg(long, default_value_t = 5.0)]
        seconds: f64,
        /// e.g. 192.168.1
        #[arg(long)]
        scan: Option<String>,
    },
    /// Model, firmware, preset, temperatures
    Status,
    /// List the parameters this amp supports
    Params {
        /// Only names starting with this (e.g. out1.eq)
        prefix: Option<String>,
    },
    /// Read parameters: exact names or prefixes (out1.eq.3, in1)
    Get {
        #[arg(required = true)]
        names: Vec<String>,
    },
    /// Write a parameter (confirmed by the amp), e.g. `set out1.gain -3`
    Set {
        name: String,
        #[arg(allow_negative_numbers = true)]
        value: String,
    },
    /// Live meters
    Meters {
        #[arg(long, default_value_t = 30.0)]
        seconds: f64,
    },
    /// Read all settings as JSON
    Dump {
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Write the settings of a JSON dump back to the amp
    Restore {
        file: PathBuf,
        /// Only report what would be written
        #[arg(long)]
        dry_run: bool,
    },
    /// Presets stored on the amp
    Presets,
    /// Load a stored preset (changes the audio, including mutes)
    Recall { index: u32 },
    /// Store the current settings in a preset slot, replacing what is there
    Store { index: u32, name: String },
    /// IPD-only actions
    #[cfg(feature = "ipd")]
    Ipd {
        #[command(subcommand)]
        cmd: IpdCmd,
    },
    /// IPX-only actions
    #[cfg(feature = "ipx")]
    Ipx {
        #[command(subcommand)]
        cmd: IpxCmd,
    },
    /// Run the IPD simulator
    #[cfg(feature = "ipd")]
    Sim {
        #[arg(long, default_value_t = 50675)]
        port: u16,
        #[arg(long, default_value = "127.0.0.1")]
        bind: String,
        /// Behave like an older MK1 unit
        #[arg(long)]
        mk1: bool,
        /// 0 IPD2400, 1 IPD1200, 2 IPD4800
        #[arg(long, default_value_t = 0)]
        model: usize,
    },
}

#[cfg(feature = "ipd")]
#[derive(Subcommand)]
enum IpdCmd {
    /// Log in and show the resulting user level
    Login {
        password: String,
    },
    Standby {
        state: String,
    },
    /// Blink the unit to find it
    Locate,
    /// Raw key access: struct member channel num
    Raw {
        #[arg(num_args = 4, value_names = ["STRUCT", "MEMBER", "CHANNEL", "NUM"])]
        key: Vec<u8>,
        /// Write this integer instead of reading
        #[arg(long)]
        set: Option<i64>,
    },
}

#[cfg(feature = "ipx")]
#[derive(Subcommand)]
enum IpxCmd {
    /// Raw parameter access: env channel item field
    Raw {
        #[arg(num_args = 4, value_names = ["ENV", "CHANNEL", "ITEM", "FIELD"])]
        addr: Vec<u8>,
        /// Write a 32-bit integer instead of reading
        #[arg(long, allow_negative_numbers = true, conflicts_with = "float")]
        int: Option<i32>,
        /// Write a 32-bit float instead of reading
        #[arg(long, allow_negative_numbers = true)]
        float: Option<f32>,
    },
}

type Res<T> = Result<T, Box<dyn std::error::Error>>;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let level = ["warn", "info", "debug"][cli.verbose.min(2) as usize];
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| level.into()),
        )
        .with_writer(std::io::stderr)
        .init();
    if let Err(e) = run(cli).await {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

async fn connect(cli: &Cli) -> Res<Arc<dyn Amp>> {
    let host = cli.host.as_deref().ok_or("--host (or $LABGRUPPEN_HOST) is required")?;
    let driver = cli.driver.as_deref().and_then(Driver::from_name);
    // host:port selects a non-default control port (needs --driver), e.g. for the simulator
    if let Some((h, port)) = host.rsplit_once(':').and_then(|(h, p)| Some((h, p.parse::<u16>().ok()?))) {
        let timeout = Duration::from_secs(10);
        return match driver.ok_or("--driver is required with host:port")? {
            #[cfg(feature = "ipx")]
            Driver::Ipx => Ok(Arc::new(labgruppen::amps::ipx::IpxAmp::connect_with(h, port, timeout).await?)),
            #[cfg(feature = "ipd")]
            Driver::Ipd => {
                let opts = labgruppen::amps::ipd::client::Options { mk2: None, timeout };
                Ok(Arc::new(labgruppen::amps::ipd::IpdAmp::connect_with(h, port, opts).await?))
            }
            #[allow(unreachable_patterns)]
            d => Err(format!("driver {} is not compiled in", d.name()).into()),
        };
    }
    Ok(labgruppen::connect(host, driver).await?)
}

fn parse_value(s: &str) -> Value {
    if let Ok(i) = s.parse::<i64>() {
        Value::Int(i)
    } else if let Ok(f) = s.parse::<f64>() {
        Value::Float(f)
    } else if let Some(b) = ["on", "true", "yes"]
        .contains(&s)
        .then_some(true)
        .or(["off", "false", "no"].contains(&s).then_some(false))
    {
        Value::Bool(b)
    } else {
        Value::Text(s.to_string())
    }
}

fn matching(amp: &dyn Amp, pattern: &str) -> Vec<Param> {
    amp.params()
        .into_iter()
        .filter(|p| {
            let n = p.to_string();
            n == pattern || n.strip_prefix(pattern).is_some_and(|rest| rest.starts_with('.'))
        })
        .collect()
}

async fn run(cli: Cli) -> Res<()> {
    match &cli.cmd {
        Cmd::Discover { seconds, scan } => return discover(*seconds, scan.as_deref()).await,
        #[cfg(feature = "ipd")]
        Cmd::Sim { port, bind, mk1, model } => {
            let addr = format!("{bind}:{port}").parse()?;
            let srv = labgruppen::amps::ipd::sim::SimServer::spawn(addr, !mk1, *model).await?;
            println!("IPD simulator on {} ({}). Ctrl-C to stop.", srv.addr, if *mk1 { "MK1" } else { "MK2" });
            tokio::signal::ctrl_c().await?;
            return Ok(());
        }
        _ => {}
    }

    let amp = connect(&cli).await?;
    let result = amp_command(&cli, amp.as_ref()).await;
    amp.close().await;
    result
}

async fn amp_command(cli: &Cli, amp: &dyn Amp) -> Res<()> {
    match &cli.cmd {
        Cmd::Status => {
            let st = amp.status().await?;
            println!("{}  name '{}'  firmware {}  [{}]", st.model, st.name, st.firmware, st.driver);
            if let Some(p) = &st.preset {
                println!("preset #{} '{}'", p.index, p.name);
            }
            if !st.temperatures.is_empty() {
                println!("temperatures {:?}", st.temperatures);
            }
            for (k, v) in &st.extra {
                println!("{k}: {v}");
            }
        }
        Cmd::Params { prefix } => {
            for p in amp.params() {
                let name = p.to_string();
                if prefix.as_ref().is_some_and(|x| !name.starts_with(x.as_str())) {
                    continue;
                }
                let s = amp.spec(&p).ok_or("spec missing")?;
                let range = match (s.min, s.max) {
                    (Some(a), Some(b)) => format!("[{a}, {b}]"),
                    _ => String::new(),
                };
                let kind = match s.kind {
                    labgruppen::Kind::Enum(l) => l.join("|"),
                    k => format!("{k:?}").to_lowercase(),
                };
                println!(
                    "{name:32} {kind} {range} {}{}",
                    s.unit,
                    if s.writable { "" } else { " (read-only)" }
                );
            }
        }
        Cmd::Get { names } => {
            let mut wanted = Vec::new();
            for n in names {
                let found = matching(amp, n);
                if found.is_empty() {
                    return Err(format!("no parameter matches {n:?} (see `params`)").into());
                }
                wanted.extend(found);
            }
            let values = amp.get_many(&wanted).await?;
            for (p, v) in wanted.iter().zip(values) {
                println!("{p} = {v}");
            }
        }
        Cmd::Set { name, value } => {
            let p: Param = name.parse()?;
            amp.set(&p, parse_value(value)).await?;
            println!("{p} = {}", amp.get(&p).await?);
        }
        Cmd::Meters { seconds } => {
            let end = Instant::now() + Duration::from_secs_f64(*seconds);
            while Instant::now() < end {
                print!("\x1b[H\x1b[J{}", format_meters(&amp.meters()));
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
        Cmd::Dump { out } => {
            let json = serde_json::to_string_pretty(&amp.snapshot().await?)?;
            match out {
                Some(path) => std::fs::write(path, json + "\n")?,
                None => println!("{json}"),
            }
        }
        Cmd::Restore { file, dry_run } => {
            let snap: Snapshot = serde_json::from_str(&std::fs::read_to_string(file)?)?;
            let info = amp.info();
            if snap.driver != info.driver {
                eprintln!(
                    "note: snapshot is from a {} amp, this is {}; only matching parameters are written",
                    snap.driver, info.driver
                );
            }
            let report = amp.restore(&snap, *dry_run).await?;
            println!("{} {} parameters", if *dry_run { "would write" } else { "wrote" }, report.applied);
            if !report.skipped.is_empty() {
                println!("skipped {} (unsupported or read-only)", report.skipped.len());
            }
            for (name, why) in &report.failed {
                println!("failed {name}: {why}");
            }
            if !report.failed.is_empty() {
                return Err(format!("{} parameters could not be written", report.failed.len()).into());
            }
        }
        Cmd::Presets => {
            for p in amp.presets().await? {
                println!("{:3} {}", p.index, p.name);
            }
        }
        Cmd::Recall { index } => {
            amp.recall_preset(*index).await?;
            println!("recalled preset {index}");
        }
        Cmd::Store { index, name } => {
            amp.store_preset(*index, name).await?;
            println!("stored preset {index} '{name}'");
        }
        #[cfg(feature = "ipd")]
        Cmd::Ipd { cmd } => ipd_command(amp, cmd).await?,
        #[cfg(feature = "ipx")]
        Cmd::Ipx { cmd } => ipx_command(amp, cmd).await?,
        Cmd::Discover { .. } => unreachable!("handled before connecting"),
        #[cfg(feature = "ipd")]
        Cmd::Sim { .. } => unreachable!("handled before connecting"),
    }
    Ok(())
}

fn format_meters(m: &Meters) -> String {
    fn bar(db: f64) -> String {
        let n = (((db + 60.0) / 60.0 * 30.0).clamp(0.0, 30.0)) as usize;
        format!("{}{}", "#".repeat(n), ".".repeat(30 - n))
    }
    let mut s = String::new();
    for (label, vals) in [("in ", &m.input), ("out", &m.output)] {
        for (i, v) in vals.iter().enumerate() {
            s += &format!("{label} {} {v:7.1} dB {}\n", i + 1, bar(*v));
        }
    }
    for (name, vals) in &m.extra {
        let v: Vec<String> = vals.iter().map(|x| format!("{x:.2}")).collect();
        s += &format!("{name}: {}\n", v.join("  "));
    }
    s
}

async fn discover(seconds: f64, scan: Option<&str>) -> Res<()> {
    let limit = Duration::from_secs_f64(seconds);
    #[cfg(feature = "ipx")]
    let ipx = async {
        match labgruppen::amps::ipx::discover(limit).await {
            Ok(found) => {
                for d in found {
                    println!("ipx  {}  {}", d.source, serde_json::to_string(&d).unwrap_or_default());
                }
            }
            Err(e) => eprintln!("ipx discovery: {e}"),
        }
        if let Some(subnet) = scan {
            for (ip, model) in labgruppen::amps::ipx::scan(subnet, Duration::from_secs(3)).await {
                println!("ipx  {ip}  {model}");
            }
        }
    };
    #[cfg(not(feature = "ipx"))]
    let ipx = async {};
    #[cfg(feature = "ipd")]
    let ipd = async {
        match labgruppen::amps::ipd::listen(limit).await {
            Ok(found) => {
                for (ip, a) in found {
                    println!("ipd  {ip}  {}", serde_json::to_string(&a).unwrap_or_default());
                }
            }
            Err(e) => eprintln!("ipd announcements: {e}"),
        }
        if let Some(subnet) = scan {
            for (ip, a) in labgruppen::amps::ipd::scan(subnet, Duration::from_secs(3)).await {
                let gen_ = if a.mk2 { "MK2" } else { "MK1" };
                println!(
                    "ipd  {ip}  {gen_}{}",
                    if a.unit_type.is_empty() { " (no unit info in probe reply)" } else { "" }
                );
            }
        }
    };
    #[cfg(not(feature = "ipd"))]
    let ipd = async {
        let _ = scan;
    };
    tokio::join!(ipx, ipd);
    Ok(())
}

#[cfg(feature = "ipd")]
async fn ipd_command(amp: &dyn Amp, cmd: &IpdCmd) -> Res<()> {
    use labgruppen::amps::ipd::{
        IpdAmp,
        protocol::{Key, Raw},
    };
    let ipd = amp.as_any().downcast_ref::<IpdAmp>().ok_or("this is not an IPD amp")?;
    match cmd {
        IpdCmd::Login { password } => println!("user: {}", ipd.login(password).await?.user),
        IpdCmd::Standby { state } => ipd.standby(state == "on").await?,
        IpdCmd::Locate => ipd.locate().await?,
        IpdCmd::Raw { key, set } => {
            let key = Key::new(key[0], key[1], key[2], key[3]);
            if let Some(v) = set {
                ipd.client().set_raw(key, Raw::Int(*v), None).await?;
            }
            println!("{key} = {:?}", ipd.client().get_raw(key, None).await?);
        }
    }
    Ok(())
}

#[cfg(feature = "ipx")]
async fn ipx_command(amp: &dyn Amp, cmd: &IpxCmd) -> Res<()> {
    use labgruppen::amps::ipx::{IpxAmp, protocol::Address};
    let ipx = amp.as_any().downcast_ref::<IpxAmp>().ok_or("this is not an IPX amp")?;
    match cmd {
        IpxCmd::Raw { addr, int, float } => {
            let a = Address::new(addr[0], addr[1], addr[2], addr[3]);
            match (int, float) {
                (Some(i), _) => ipx.set_raw(a, &i.to_le_bytes()).await?,
                (_, Some(f)) => ipx.set_raw(a, &f.to_le_bytes()).await?,
                _ => {}
            }
            let raw = ipx.get_raw(a).await?;
            match <[u8; 4]>::try_from(raw.as_slice()) {
                Ok(b) => {
                    println!("{:?} = int {} / float {}", addr, i32::from_le_bytes(b), f32::from_le_bytes(b))
                }
                Err(_) => println!(
                    "{:?} = {} bytes: {}",
                    addr,
                    raw.len(),
                    raw.iter().map(|x| format!("{x:02x}")).collect::<String>()
                ),
            }
        }
    }
    Ok(())
}
