# Axolote Framework

[![Language](https://img.shields.io/badge/Language-Rust-orange.svg)](https://rust-lang.org)
[![Dependencies](https://img.shields.io/badge/Dependencies-0-brightgreen.svg)]()
[![Cargo](https://img.shields.io/badge/Cargo-Not_Required-blue.svg)]()
[![License](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)

*English | [Português](README-pt-BR.md)*

**Axolote** is a pure stdlib HTTP and WebSocket framework written in Rust with **zero external dependencies** (no Cargo, no Tokio, no Serde, no third-party crates).

The entire technical foundation — routing, request parsing, JSON serialization, logging, concurrency, and the WebSocket protocol (including S2S clustering) — was built from scratch using only the Rust standard library (`std`) and raw `libc` syscalls (e.g., `epoll`).

## Quick Glance

```rust
extern crate axolote;
use axolote::Server;
use axolote::http::{HttpMethod, HttpRequest, HttpResponse};

fn main() {
    let mut server = Server::new("8080");
    
    server.add_route(HttpMethod::GET, "/", |_req: HttpRequest| {
        HttpResponse::ok("Hello World from Axolote!")
    });

    server.run();
}
```

## Architecture & Features

The project is divided into two main modules:

- **[HTTP Module](docs/http/README.md)**: Dynamic routing, path/query extraction, middlewares, non-blocking I/O with custom `epoll` reactor, custom JSON parser `#[axolote_json]` (without `serde`), and thread-pool based concurrency.
- **[WebSocket Module](docs/websocket/README.md)**: From-scratch RFC 6455 implementation (framing, masking, SHA-1 handshake), O(1) Sharding Pub/Sub Hub, rooms, and a **Decentralized S2S Cluster** using a Gossip Mesh Relay protocol.

## Building the Framework

The project is designed to be compiled directly via the command line (invoking `rustc` natively) using a `Makefile`. No Cargo required.

### 1. Build the Core Library
Compile the engine and generate the static library (`.rlib`):
```bash
make build-lib
```

### 2. Build Examples
The `/examples` directory contains practical use cases. To compile all of them:
```bash
make examples
```

### 3. Clean
```bash
make clean
```

## Using Axolote in Your Projects

Because it has no dependencies, you link the compiled library directly.

1. Grab the `libaxolote.rlib` file.
2. In your code, declare `extern crate axolote;`.
3. Compile with `rustc main.rs --extern axolote=libaxolote.rlib`.

**Using with Cargo:**
If your target project uses Cargo, you can drop `libaxolote.rlib` and the provided **`build.rs`** into your project root. Cargo will automatically link it.

## Documentation & Examples

Detailed architecture guides and ready-to-run examples can be found in the [`docs/`](docs/) directory:

- **HTTP**: [Guide](docs/http/README.md) | [Examples](docs/http/examples/)
- **WebSocket**: [Guide](docs/websocket/README.md) | [Security](docs/websocket/SECURITY.md) | [Gossip Mesh Cluster](docs/websocket/CLUSTER.md) | [Examples](docs/websocket/examples/)

## License

MIT License. See `LICENSE` for more information.
