# Rtget

## Disclaimer
This is a personal project aimed at learning Rust

## Description

This application is a Rust-based clone of the classic `wget` utility, designed for downloading content from the web. It supports HTTP/HTTPS and FTP/FTPS protocols and features concurrent downloading capabilities.

## Features

- Supports downloading via HTTP/HTTPS and FTP/FTPS.
- Concurrent downloads for efficient file retrieval.
- Command-line interface for ease of use.
- Optional background operation mode (on Unix based systems).
- Progress display for tracking download status.

## Installation

### Prerequisites

- Rust (latest stable version recommended)
- Cargo (Rust's package manager)

### Steps

1. Clone the repository:
   ```bash
   git clone https://github.com/carthage84/rtget.git
   ```
2. Navigate to the cloned directory:
   ```bash
   cd rtget
   ```
3. Build the project using Cargo:
   ```bash
   cargo build --release
   ```
4. The executable will be located in `target/release/`.

## Usage

To use the application, run the executable from the command line with the desired options. For example:

```bash
./rtget -u [URL] -o [output path] -c [number of connections] [-b]
```

### Options

- `-u`, `--url`: The URL to download.
- `-o`, `--output`: (Optional) Output file path.
- `-c`, `--connections`: (Optional) Number of concurrent connections. Default is 4.
- `-b`, `--background`: (Optional) Run in the background.

## Contributing

Contributions to the project are welcome! Please refer to the `CONTRIBUTING.md` file for guidelines.

## License

This project is licensed under the GNU GPLv3 — see the `LICENSE` file for details.

## Acknowledgments

- Thanks to all the contributors who have helped with this project.
- Special thanks to Tim McNamara and the Rust community for their invaluable support and resources.