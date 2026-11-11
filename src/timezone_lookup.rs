use anyhow::{bail, Context};
use anyhow::Result as AnResult;
use log::debug;
use serde_json::Value;
use std::cmp::Reverse;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};
use tz::TimeZone;
use std::fmt::{self, Display, Formatter};

struct GeoIPService {
	url: &'static str,
	tz_keys: &'static [&'static str],
}

struct Tally {
	count: usize,
	first_seen: usize,
}

struct SpawnedRequests {
	receiver: Receiver<String>,
	svc_count: usize,
}

/// {ip} will be replaced with the IP. Services must be able to take {ip} being replaced with "" as
/// meaning to use the source IP for the request.
static SERVICES: &[GeoIPService] = &[
	GeoIPService {
		url: "https://geoip.chrisdown.name/{ip}",
		tz_keys: &["location", "time_zone"],
	},
	GeoIPService {
		url: "https://api.ipbase.com/v1/json/{ip}",
		tz_keys: &["time_zone"],
	},
	GeoIPService {
		url: "https://ipapi.co/{ip}/json/",
		tz_keys: &["timezone"],
	},
	GeoIPService {
		url: "https://worldtimeapi.org/api/ip/{ip}",
		tz_keys: &["timezone"],
	},
	GeoIPService {
		url: "https://reallyfreegeoip.org/json/{ip}",
		tz_keys: &["time_zone"],
	},
	GeoIPService {
		url: "https://ipwho.is/{ip}",
		tz_keys: &["timezone", "id"],
	},
	GeoIPService {
		url: "https://ipinfo.io/{ip}/json",
		tz_keys: &["timezone"],
	},
];

/// Given &["foo", "bar", "baz"], retrieve the value at data["foo"]["bar"]["baz"].
fn get_nested_value(mut data: Value, keys: &[&str]) -> Option<Value> {
	for key in keys {
		match data {
			Value::Object(mut map) => {
				data = map.remove(*key)?;
			}
			_ => return None,
		}
	}

	Some(data)
}

/// A single service worker, racing with others as part of `get_timezone`.
fn get_timezone_for_ip(url: &str, service: &GeoIPService, sender: Sender<String>) -> AnResult<()> {
	let mut res = ureq::get(url).call()?;
	let val = match get_nested_value(res.body_mut().read_json()?, service.tz_keys)
		.with_context(|| format!("Invalid data for {url}"))?
	{
		Value::String(s) => s,
		_ => bail!("Timezone field for {url} is not a string"),
	};
	debug!("Sending {val} back to main thread from {url}");

	// Only fails if receiver is disconnected, which just means we lost the race
	let _ = sender.send(val);
	Ok(())
}

/// Spawn workers for all SERVICES and return a handle for their responses.
fn spawn_requests(ip_addr: Option<&IpAddr>) -> SpawnedRequests {
	let (sender, receiver) = channel();

	let ip_addr = ip_addr.map(IpAddr::to_string);
	for (svc, sender) in SERVICES.iter().zip(std::iter::repeat(sender)) {
		let url = svc.url.replace("{ip}", ip_addr.as_deref().unwrap_or(""));
		// For our small number of services, this makes more sense than using a full async runtime
		thread::spawn(move || {
			if let Err(err) = get_timezone_for_ip(&url, svc, sender) {
				debug!("{url}: {err}");
			}
		});
	}

	SpawnedRequests {
		receiver,
		svc_count: SERVICES.len(),
	}
}



pub enum TimezoneLookupStrategy {
	FirstSuccessful,
	MostFrequent,
}

pub enum PossibleTimezone {
	KnownTimezone {
		identifier: String,
		tzdata: TimeZone,
	},
	UnknownTimezone(String),
}

impl PossibleTimezone {
    pub fn identifier(&self) -> &str {
        match self {
            Self::KnownTimezone { identifier, .. } => identifier,
            Self::UnknownTimezone(identifier) => identifier,
        }
    }

    pub fn tzdata(&self) -> Option<&TimeZone> {
        match self {
            Self::KnownTimezone { tzdata: timezone, .. } => Some(timezone),
            Self::UnknownTimezone(_) => None,
        }
    }

    pub fn is_known(&self) -> bool {
        matches!(self, Self::KnownTimezone { .. })
    }
}

impl Display for PossibleTimezone {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		match self {
			Self::KnownTimezone { identifier: value, .. } | Self::UnknownTimezone(value) => {
				f.write_str(value)
			}
		}
	}
}

impl From<&str> for PossibleTimezone {
	fn from(value: &str) -> Self {
		match TimeZone::from_posix_tz(value) {
			Ok(timezone) => Self::KnownTimezone {
				tzdata: timezone,
				identifier: value.to_owned(),
			},
			Err(_) => Self::UnknownTimezone(value.to_owned()),
		}
	}
}

impl From<String> for PossibleTimezone {
	fn from(value: String) -> Self {
		match TimeZone::from_posix_tz(&value) {
			Ok(timezone) => Self::KnownTimezone {
				tzdata: timezone,
				identifier: value,
			},
			Err(_) => Self::UnknownTimezone(value),
		}
	}
}


#[derive(Debug, thiserror::Error)]
pub enum TimezoneLookupError {
	#[error("All APIs failed. Run with RUST_LOG=tzupdate=debug for more information.")]
	AllApisFailed,

	#[error("All APIs timed out, consider increasing --timeout.")]
	AllApisTimedOut,
}


/// Spawn background HTTP requests, returning the first timezone that replies.
fn get_timezone_first(
	timeout: Duration,
	ip_to_lookup: Option<&IpAddr>,
) -> Result<PossibleTimezone, TimezoneLookupError> {
	let SpawnedRequests {
		receiver,
		..
	} = spawn_requests(ip_to_lookup);

	receiver
		.recv_timeout(timeout)
		.map_err(|err| match err {
			RecvTimeoutError::Disconnected => TimezoneLookupError::AllApisFailed,
			RecvTimeoutError::Timeout => TimezoneLookupError::AllApisTimedOut,
		})
		.map(PossibleTimezone::from)
}



/// Spawn background HTTP requests, collecting responses and choosing by consensus.
/// If a strict majority (> 50%) is reached before the timeout, return immediately.
/// Otherwise, after the timeout or when all responses are in, return the most frequent
/// timezone seen. Ties are broken by the earliest response among the tied values.
fn get_timezone_consensus(
	timeout: Duration,
	ip_to_lookup: Option<&IpAddr>,
) -> Result<PossibleTimezone, TimezoneLookupError> {
	let SpawnedRequests {
		receiver,
		svc_count,
	} = spawn_requests(ip_to_lookup);

	let deadline = Instant::now() + timeout;
	let majority = svc_count / 2 + 1;

	let mut tallies = HashMap::new();
	let mut seen_idx = 0usize;
	let mut timed_out = false;

	loop {
		let now = Instant::now();
		if now >= deadline {
			timed_out = true;
			break;
		}

		match receiver.recv_timeout(deadline.saturating_duration_since(now)) {
			Ok(tz) => {
				seen_idx += 1;
				let entry = tallies.entry(tz.clone()).or_insert(Tally {
					count: 0,
					first_seen: seen_idx,
				});
				entry.count += 1;

				if entry.count >= majority {
					return Ok(PossibleTimezone::from(tz));
				}

				if seen_idx == svc_count {
					break;
				}
			}
			Err(RecvTimeoutError::Timeout) => {
				timed_out = true;
				break;
			}
			Err(RecvTimeoutError::Disconnected) => break,
		}
	}

	if tallies.is_empty() {
		return Err(if timed_out {
			TimezoneLookupError::AllApisTimedOut
		} else {
			TimezoneLookupError::AllApisFailed
		});
	}

	let (best_tz, _) = tallies
		.into_iter()
		.max_by_key(|(_, tally)| (tally.count, Reverse(tally.first_seen)))
		.unwrap();

	Ok(PossibleTimezone::from(best_tz))
}



pub fn get_timezone(
	mode:TimezoneLookupStrategy,
	timeout: Duration,
	ip_to_lookup: Option<&IpAddr>,
) -> Result<PossibleTimezone, TimezoneLookupError> {
	match mode {
		TimezoneLookupStrategy::FirstSuccessful => {
			get_timezone_first(timeout, ip_to_lookup)
		}
		TimezoneLookupStrategy::MostFrequent => {
			get_timezone_consensus(timeout, ip_to_lookup)
		}
	}
}



