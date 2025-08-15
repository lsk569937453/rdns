# RDNS

A lightweight, asynchronous DNS forwarder written in Rust. This server can resolve DNS queries from a local, in-memory cache or forward them to multiple upstream DNS servers concurrently. It's designed to be simple, efficient, and easy to configure.

## Features

- **Local DNS Records**: Serve custom DNS records directly from a configuration file.
- **Concurrent Forwarding**: Forwards queries to multiple upstream servers and uses the response from the fastest one.
- **Asynchronous**: Built with Tokio and `async_trait` for high-performance, non-blocking I/O.
- **Timeout Handling**: Implements a query timeout to prevent requests from hanging.
- **Simple Configuration**: Easy to set up local records and upstream servers via a TOML file.
- **Robust Error Handling**: Gracefully handles I/O errors and invalid DNS requests.

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (latest stable version recommended)

## Installation

1.  **Clone the repository:**

    ```bash
    git clone git@github.com:lsk569937453/rdns.git
    cd rdns
    ```

2.  **Build the project:**
    ```bash
    cargo build --release
    ```
    The compiled binary will be located in `target/release/`.

## Configuration

The server is configured using a `config.toml` file. Create this file in the root of the project directory.

Here is an example configuration:

```yml
port: 4325

upstream_servers:
  - "223.5.5.5:53"
  - "8.8.8.8:53"
  - "1.1.1.1:53"

records:
  "example.com.":
    type: "A"
    value: "127.0.0.1"

  "www.example.com.":
    type: "CNAME"
    value: "example.com."

  "localhost.":
    type: "A"
    value: "127.0.0.1"

  "ipv6.example.com.":
    type: "AAAA"
    value: "::1"
```

### Configuration Details

- `port`: The UDP port for the server to listen on.
- `upstream_servers`: A list of IP addresses and ports for upstream DNS servers. The server will forward queries to all of them and use the first valid response.
- `[records]`: A table defining your local DNS records.
  - Each key is the domain name (make sure to include the trailing dot).
  - `type`: The record type (e.g., "A", "AAAA", "CNAME", "MX", etc.).
  - `value`: The value of the record (an IP address for "A" records, another domain for "CNAME", etc.).

## Usage

1.  **Run the server:**

    ```bash
    cargo run --release
    ```

2.  **Test the server** using a DNS client like `dig` or `nslookup`.

    **Query a local record:**

    ```bash
    dig @127.0.0.1 -p 5353 home.local.
    ```

    **Query an external record (will be forwarded):**

    ```bash
    dig @127.0.0.1 -p 5353 google.com
    ```

## How to Contribute

Contributions are welcome! Please feel free to open an issue or submit a pull request.

1.  Fork the repository.
2.  Create a new branch (`git checkout -b feature/your-feature`).
3.  Make your changes.
4.  Commit your changes (`git commit -am 'Add some feature'`).
5.  Push to the branch (`git push origin feature/your-feature`).
6.  Create a new Pull Request.
