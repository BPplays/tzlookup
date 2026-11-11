# tzlookup

tzlookup is a fully automated utility to set the system time using geolocation.  
forked from [tzupdate](https://github.com/cdown/tzupdate)

## Features

- Small, easy to understand codebase
- Queries multiple geolocation services in parallel and returns the first with
  a successful result
- Protects against directory traversal and invalid results when linking
  /etc/localtime

## Installation

    cargo add tzlookup

## Usage


```rust
use std::time::Duration;
use tzlookup::lookup;
use tzlookup::file;

fn main() {
    let tz = lookup::get_timezone(
        lookup::TimezoneLookupStrategy::MostFrequent,
        Duration::from_secs(5),
        None, // don't specify an IP to lookup
    ).unwrap();
    println!("{tz}");

    file::set_timezone(
        tz.identifier(),
        "/etc/localtime",
        "/usr/share/zoneinfo",
        "/etc/timezone",
        false,
    )?;
}
```

or the systemd version (requires the `systemd` feature)
```rust
use std::time::Duration;
use tzlookup::lookup;

// requires the feature 'systemd'
use tzlookup::systemd;

fn main() {

    let tz = lookup::get_timezone(
        lookup::TimezoneLookupStrategy::MostFrequent,
        Duration::from_secs(5),
        None, // don't specify an IP to lookup
    ).unwrap();
    println!("{tz}");

    systemd::set_tz(tz.identifier()).await.unwrap();
}
```
