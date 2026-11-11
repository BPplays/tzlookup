# tzlookup

tzlookup is a fully automated utility to set the system time using geolocation.

## Features

- Small, easy to understand codebase
- Queries multiple geolocation services in parallel and returns the first with
  a successful result
- Protects against directory traversal and invalid results when linking
  /etc/localtime

## Installation

    cargo install tzlookup

## Add to Your Project

    cargo add tzlookup

## Usage

    -> tzupdate
    Set system timezone to Europe/London.

Internally, this geolocates you, gets the timezone for that geolocation, and
then updates the system's local time zone.

You can see what tzupdate would do without actually doing it by passing `-p`,
and specify an alternative IP address by using `-i`. This is not an exhaustive
list of options, see `tzupdate --help` for that.

you can also use it as a library

```rust
use tzlookup::lookup
use tzlookup::file

fn main() {
    let tz = lookup::get_timezone(
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
}
```

or the systemd version (requires the 'systemd' feature)
```rust
use tzlookup::lookup

// requires the feature 'systemd'
use tzlookup::systemd

fn main() {

    let tz = lookup::get_timezone(
        mode,
        cfg.timeout,
        cfg.ip.as_ref(),
    ).unwrap();
    println!("{tz}");

    systemd::set_tz(tz.identifier()).await.unwrap();
}
```
