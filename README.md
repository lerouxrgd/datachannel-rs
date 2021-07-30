# datachannel-rs &emsp; [![latest]][crates.io] [![doc]][docs.rs]

[latest]: https://img.shields.io/crates/v/datachannel.svg
[crates.io]: https://crates.io/crates/datachannel
[doc]: https://docs.rs/datachannel/badge.svg
[docs.rs]: https://docs.rs/datachannel

Rust wrappers for [libdatachannel][], a WebRTC Data Channels standalone implementation in C++.

## Usage

This crate provides two traits that end user must implement, `DataChannelHandler` and
`PeerConnectionHandler`, which defined all callbacks for `RtcPeerConnection` and
`RtcDataChannel` structs respectively.

Aforementioned traits are defined as follows:

```rust
pub trait DataChannelHandler {
    fn on_open(&mut self) {}
    fn on_closed(&mut self) {}
    fn on_error(&mut self, err: &str) {}
    fn on_message(&mut self, msg: &[u8]) {}
    fn on_buffered_amount_low(&mut self) {}
    fn on_available(&mut self) {}
}

pub trait PeerConnectionHandler {
    type DCH;

    fn data_channel_handler(&mut self) -> Self::DCH;

    fn on_description(&mut self, sess_desc: SessionDescription) {}
    fn on_candidate(&mut self, cand: IceCandidate) {}
    fn on_connection_state_change(&mut self, state: ConnectionState) {}
    fn on_gathering_state_change(&mut self, state: GatheringState) {}
    fn on_data_channel(&mut self, data_channel: Box<RtcDataChannel<Self::DCH>>) {}
}
```

Note that all `on_*` methods have a default no-operation implementation.

The main struct, `RtcPeerconnection`, takes a `RtcConfig` (which defines ICE servers)
and a instance of `PeerConnectionHandler`.

Here is the basic workflow:

```rust
use datachannel::{DataChannelHandler, PeerConnectionHandler, RtcConfig, RtcPeerConnection};

struct MyChannel;

impl DataChannelHandler for MyChannel {
    fn on_open(&mut self) {
        // TODO: notify that the data channel is ready (optional)
    }

    fn on_message(&mut self, msg: &[u8]) {
        // TODO: process the received message
    }
}

struct MyConnection;

impl PeerConnectionHandler for MyConnection {
    type DCH = MyChannel;

    /// Used to create the `RtcDataChannel` received through `on_data_channel`.
    fn data_channel_handler(&mut self) -> Self::DCH {
        MyChannel
    }

    fn on_data_channel(&mut self, mut dc: Box<RtcDataChannel<Self::DCH>>) {
        // TODO: store `dc` to keep receiving its messages (otherwise it will be dropped)
    }
}

let ice_servers = vec!["stun:stun.l.google.com:19302"];
let conf = RtcConfig::new(&ice_servers);

let mut pc = RtcPeerConnection::new(&conf, MyConnection)?;

let mut dc = pc.create_data_channel("test-dc", MyChannel)?;
// TODO: exchange `SessionDescription` and `IceCandidate` with remote peer
// TODO: wait for `dc` to be opened (should be signaled through `on_open`)
// ...
// Then send a message
dc.send("Hello Peer!".as_bytes())?;
```

Complete implementation example can be found in the [tests](tests).

## Building

Note that `CMake` is required to compile [libdatachannel][] through
[datachannel-sys](datachannel-sys).

### Static build

By default [libdatachannel][] will be built and linked dynamically. However there is a
`static` Cargo feature that will build and link it statically (with all its
dependencies, including `OpenSSL`).

### Apple macOS

You probably need to set the following environment variables if your build fails with an
`OpenSSL` related error.

```bash
export OPENSSL_ROOT_DIR=/usr/local/Cellar/openssl@1.1/1.1.1i/
export OPENSSL_LIBRARIES=/usr/local/Cellar/openssl@1.1/1.1.1i/lib
```

With the paths of your local `OpenSSL` installation.

[libdatachannel]: https://github.com/paullouisageneau/libdatachannel

### Ubuntu

```sh
# Needed to compile libdatachannel
sudo apt install build-essential cmake pkg-config libssl-dev clang

# Checkout datachannel-rs
git clone https://github.com/lerouxrgd/datachannel-rs.git
cd datachannel-rs

# Recursive update submodules (libdatachanel has it's own submodules)
git submodule update --init --recursive

# Build
cargo build
```
