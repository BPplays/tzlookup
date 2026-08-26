use anyhow::{Ok, Result};
use clap::{Parser};
use log::info;
use std::path::PathBuf;
use std::time::Duration;
use std::process;

mod file;
mod http;
mod systemd;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Config {
    #[arg(
        short,
        long,
        help = "print the timezone, but don't update /etc/timezone or /etc/localtime"
    )]
    print_only: bool,

    #[arg(
        short,
        long,
        help = "use this IP instead of automatically detecting it"
    )]
    ip: Option<String>,

    #[arg(
        short,
        long,
        help = "use this timezone instead of automatically detecting it"
    )]
    timezone: Option<String>,

    #[arg(
        short,
        long,
        help = "use consensus to choose timezone instead of first response"
    )]
    consensus: bool,

    #[arg(
        short,
        long,
        help = "path to root of the zoneinfo database",
        default_value = "/usr/share/zoneinfo"
    )]
    zoneinfo_path: PathBuf,

    #[arg(
        short,
        long,
        help = "path to localtime symlink",
        default_value = "/etc/localtime"
    )]
    localtime_path: PathBuf,

    #[arg(
        short,
        long,
        help = "path to Debian timezone name file",
        default_value = "/etc/timezone"
    )]
    debian_timezone_path: PathBuf,

    #[arg(long, help = "create Debian timezone file even if it doesn't exist")]
    always_write_debian_timezone: bool,

    #[arg(
        short = 's',
        long,
        help = "maximum number of seconds to wait for APIs to return",
        value_parser = parse_secs,
        default_value = "30"
    )]
    timeout: Duration,

    #[arg(
        short = 'y',
        long,
        help = "use systemd to set the timezone instead of build-in method",
    )]
    systemd: bool,

    #[arg(
        short,
        long,
        help = "test if feature is supported by returning code 0, else returning 1"
    )]
    feature_test: Option<String>,

    #[arg(
        long,
        help = "list all features",
    )]
    features_list: bool,

    #[arg(
        long,
        help = "separator for the features list",
        default_value = ","
    )]
    features_list_separator: String,
}

const FEATURES_SUPPORTED: &[&str] = &["systemd", "consensus"];

fn list_features(sep: &str) -> String {
    FEATURES_SUPPORTED.join(sep)
}

fn parse_secs(arg: &str) -> Result<Duration> {
    Ok(Duration::from_secs(arg.parse()?))
}

fn return_supported_feat(feat: &str) {
    if FEATURES_SUPPORTED.contains(&feat.to_lowercase().as_str()) {
        process::exit(0);
    }

    process::exit(1);
}

fn set_tz(tz: &str, cfg: Config) -> Result<()> {
    if cfg.print_only { return Ok(()) }

    if cfg.systemd {
        systemd::set_tz(tz)?;
    } else {
        file::link_localtime(
            tz,
            cfg.localtime_path,
            cfg.zoneinfo_path,
        )?;
        file::write_timezone(
            tz,
            cfg.debian_timezone_path,
            cfg.always_write_debian_timezone,
        )?;
    }
    return Ok(());
}

fn main() -> Result<()> {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("warn"));

    let cfg = Config::parse();

    if cfg.features_list {
        let features = list_features(&cfg.features_list_separator);
        println!("{features}");
        return Ok(());
    }

    if let Some(feature) = cfg.feature_test.as_deref().filter(|s| {
        !s.trim().is_empty()
    }) {
        return_supported_feat(feature)
    }

    let tz = match cfg.timezone.clone() {
        Some(tz) => tz,
        None => {
            if cfg.consensus {
                http::get_timezone_consensus(
                    cfg.ip.clone().unwrap_or_default(),
                    cfg.timeout,
                )?
            } else {
                http::get_timezone_first(
                    cfg.ip.clone().unwrap_or_default(),
                    cfg.timeout,
                )?
            }
        }
    };

    if cfg.print_only {
        println!("{tz}");
        return Ok(());
    }

    info!("Got timezone {tz}");
    set_tz(&tz, cfg)?;
    println!("Set system timezone to {tz}.");

    Ok(())
}
