use anyhow::Result as AnResult;
use anyhow::Ok as AnOk;
use clap::{Parser};
use log::info;
use std::net::IpAddr;
use std::path::PathBuf;
use std::time::Duration;
use std::process;
use strum;
use std::str::FromStr;

mod file;
mod timezone_lookup;
use timezone_lookup::PossibleTimezone;
mod systemd;

#[derive(Debug, Clone, PartialEq, Eq, strum::Display, strum::EnumString)]
#[strum(serialize_all = "kebab-case")]
enum TzSetMethod {
    None,
    Systemd,
    File,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Config {
    #[arg(
        short = 'm',
        long,
        help = "the method used for setting the timezone",
        default_value_t = TzSetMethod::None,
        value_parser = clap::value_parser!(TzSetMethod),
    )]
    set_method: TzSetMethod,

    #[arg(
        short,
        long,
        help = "use this IP instead of automatically detecting it"
    )]
    ip: Option<IpAddr>,

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

fn parse_secs(arg: &str) -> AnResult<Duration> {
    AnOk(Duration::from_secs(arg.parse()?))
}

fn return_supported_feat(feat: &str) {
    if FEATURES_SUPPORTED.contains(&feat.to_lowercase().as_str()) {
        process::exit(0);
    }

    process::exit(1);
}

#[tokio::main]
async fn main() -> AnResult<()> {


    let tz = timezone_lookup::get_timezone(
        mode,
        cfg.timeout,
        cfg.ip.as_ref(),
    ).unwrap();
    println!("{tz}");

    file::set_timezone(
        tz.identifier(),
        "/etc/localtime",
        "/usr/share/zoneinfo",
        "/etc/timezone",
        false,
    )?;
    systemd::set_tz(tz.identifier()).await.unwrap();


    env_logger::init_from_env(env_logger::Env::default().default_filter_or("warn"));

    let cfg = Config::parse();

    if cfg.features_list {
        let features = list_features(&cfg.features_list_separator);
        println!("{features}");
        return AnOk(());
    }

    if let Some(feature) = cfg.feature_test.as_deref().filter(|s| {
        !s.trim().is_empty()
    }) {
        return_supported_feat(feature)
    }

    let tz = match cfg.timezone.clone() {
        Some(tz) => PossibleTimezone::from(tz),
        None => {
            let mode = if cfg.consensus {
                timezone_lookup::TimezoneLookupStrategy::MostFrequent
            } else {
                timezone_lookup::TimezoneLookupStrategy::FirstSuccessful
            };

            timezone_lookup::get_timezone(
                mode,
                cfg.timeout,
                cfg.ip.as_ref(),
            )?
        }
    };

    use TzSetMethod as sm;
    match cfg.set_method {
        sm::None => {
            println!("{tz}");
        }
        sm::Systemd => {
            info!("Got timezone {tz}");
            systemd::set_tz(tz.identifier()).await?;
            println!("Set system timezone to {tz}.");
        }
        sm::File => {
            println!("{tz}");
            info!("Got timezone {tz}");
            file::set_timezone(
                tz.identifier(),
                cfg.localtime_path,
                cfg.zoneinfo_path,
                cfg.debian_timezone_path,
                cfg.always_write_debian_timezone,
            )?;
            println!("Set system timezone to {tz}.");
        }

    }


    AnOk(())
}
