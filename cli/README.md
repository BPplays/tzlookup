# tzlookup

tzlookup is a fully automated utility to set the system time using geolocation.

## Features

- Small, easy to understand codebase
- Queries multiple geolocation services in parallel and returns the first with
  a successful result
- Protects against directory traversal and invalid results when linking
  /etc/localtime

## Installation

    cargo install tzlookup-cli

## Usage

    -> tzlookup
    Set system timezone to Europe/London.

Internally, this geolocates you, gets the timezone for that geolocation, and
then updates the system's local time zone.

You can set what tzlookup will do by using `-m` with (`none`, `systemd`, `file`),
and specify an alternative IP address by using `-i`. This is not an exhaustive
list of options, see `tzlookup --help` for that.

