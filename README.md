# Rtget

A Rust clone of `wget` for downloading files over HTTP/HTTPS and FTP/FTPS.

This is a personal project aimed at learning Rust.

## Features

- HTTP/HTTPS downloads, including HTTP/2
- FTP and FTPS (explicit TLS) downloads
- Segmented downloads over multiple connections when the server supports byte ranges
- Automatic fallback to a single connection when ranges are not available
- Resume of partial downloads (`-C` / `--resume`)
- HTTP, HTTPS, SOCKS5, and SOCKS5h proxies (`--proxy`), including `HTTP CONNECT` for FTP
- Environment proxy variables (`HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`, `NO_PROXY`)
- Basic authentication (`--user` / `--password`, or credentials in the URL)
- Retries with exponential backoff
- Progress bars for each connection
- Background mode (`-b`): Unix daemonize, Windows detached process (log written to `rtget.log`)

## Installation

### Prerequisites

- Rust (latest stable)
- Cargo

### Build

```bash
git clone https://github.com/carthage84/rtget.git
cd rtget
cargo build --release
```

The executable is `target/release/rtget` (or `rtget.exe` on Windows).

## Usage

```bash
rtget [URL] [options]
rtget -u [URL] -o [output path] -c [connections]
```

Examples:

```bash
# Download a file, name taken from the URL
rtget https://example.com/file.zip

# Four connections into a chosen path
rtget -u https://example.com/file.zip -o downloads/file.zip -c 4

# Resume a partial download
rtget -C https://example.com/file.zip

# HTTP or SOCKS proxy
rtget --proxy http://127.0.0.1:8080 https://example.com/file.zip
rtget --proxy socks5h://127.0.0.1:1080 https://example.com/file.zip

# FTP with anonymous login
rtget ftp://ftp.example.com/pub/file.bin

# Background download (logs to rtget.log)
rtget -b https://example.com/large.iso
```

### Options

| Flag | Description |
| --- | --- |
| positional URL | URL to download |
| `-u`, `--url` | URL to download |
| `-o`, `--output` | Output file path (or directory) |
| `-c`, `--connections` | Concurrent connections (default 4, max 64) |
| `-C`, `--resume` | Resume a partial download |
| `-b`, `--background` | Continue in the background; log to `rtget.log` |
| `-v`, `--verbose` | Debug logging |
| `-q`, `--quiet` | No progress bar |
| `-T`, `--timeout` | Network timeout in seconds (default 30, `0` disables) |
| `-t`, `--tries` | Retries per request (default 5) |
| `-U`, `--user-agent` | Override the User-Agent string |
| `-H`, `--header` | Extra HTTP header (`Name: value`), repeatable |
| `--proxy` | Proxy URL (`http://`, `https://`, `socks5://`, `socks5h://`) |
| `--no-proxy` | Ignore `--proxy` and environment proxy settings |
| `--no-check-certificate` | Skip TLS certificate verification |
| `--user` | Username for HTTP or FTP |
| `--password` | Password for HTTP or FTP |

Run `rtget --help` for the generated help text.

## How segmented downloads work

1. Probe the URL with `HEAD`, or a `Range: bytes=0-0` GET if `HEAD` is rejected.
2. If the server honours byte ranges and the file is large enough, split it across `-c` connections.
3. Each connection writes a sibling part file (`file.bin.part.0`, …).
4. Parts are streamed together into the final file and then deleted.
5. If the server ignores `Range` and returns HTTP 200, rtget falls back to one connection.

FTP segmented downloads use the `REST` command the same way. If `REST` is not supported, a single data connection is used.

## License

GNU GPLv3 — see `LICENSE`.

## Acknowledgments

- Tim McNamara and the Rust community
- The authors of `reqwest`, `suppaftp`, `indicatif`, and `tokio`
