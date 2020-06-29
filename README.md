# datachannel-rs &emsp; [![latest]][crates.io] [![doc]][docs.rs]

[latest]: https://img.shields.io/crates/v/datachannel.svg
[crates.io]: https://crates.io/crates/datachannel
[doc]: https://docs.rs/datachannel/badge.svg
[docs.rs]: https://docs.rs/datachannel

Rust wrappers for [libdatachannel][], a WebRTC Data Channels standalone implementation in C++.

## Usage

This crate provides two traits that end user must implement, `DataChannel` and
`PeerConnection`, which define the main callback functions:

```rust
pub trait DataChannel {
    fn on_open(&mut self) {}
    fn on_closed(&mut self) {}
    fn on_error(&mut self, err: &str) {}
    fn on_message(&mut self, msg: &[u8]) {}
}

pub trait PeerConnection {
    type DC;

    fn on_description(&mut self, sess_desc: SessionDescription) {}
    fn on_candidate(&mut self, cand: IceCandidate) {}
    fn on_conn_state_change(&mut self, state: ConnectionState) {}
    fn on_gathering_state_change(&mut self, state: GatheringState) {}
    fn on_data_channel(&mut self, data_channel: Box<RtcDataChannel<Self::DC>>) {}
}
```

Custom implementations are meant to be used through `RtcPeerConnection` and
`RtcDataChannel` structs.

The main struct, `RtcPeerconnection`, takes a `Config` struct (which defines ICE
servers) and a `MakeDataChannel` instance (a factory used internally for
`on_data_channel` callback). Note that this factory trait is already implemented for
`FnMut` closures.

Complete implementation example can be found in the [tests](tests).

[libdatachannel]: https://github.com/paullouisageneau/libdatachannel
