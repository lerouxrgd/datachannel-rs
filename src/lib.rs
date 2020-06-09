use datachannel_sys as sys;

mod config;
mod datachannel;
mod peerconnection;

use crate::config::Config;
use crate::datachannel::{DataChannel, MakeDataChannel, RtcDataChannel};
use crate::peerconnection::{ConnState, GatheringConnState, PeerConnection, RtcPeerConnection};

pub fn wip() {
    use std::ptr;
    use std::sync::mpsc::{sync_channel, SyncSender};

    unsafe { sys::rtcInitLogger(5u32) };

    struct Chan(usize);

    impl DataChannel for Chan {
        fn on_message(&mut self, msg: &[u8]) {
            println!("Message {}: {}", self.0, String::from_utf8_lossy(msg));
        }
    };

    impl MakeDataChannel<Chan> for Chan {
        fn make(&self) -> Chan {
            Chan(self.0)
        }
    };

    struct PeerPair {
        id: usize,
        other: *mut PeerPair,
        conn: *mut RtcPeerConnection<PeerPair, Chan>,
    }

    impl PeerPair {
        pub fn new() -> (PeerPair, PeerPair) {
            let mut pc1 = PeerPair {
                id: 1,
                other: ptr::null_mut(),
                conn: ptr::null_mut(),
            };
            let mut pc2 = PeerPair {
                id: 2,
                other: ptr::null_mut(),
                conn: ptr::null_mut(),
            };

            pc2.other = &mut pc1 as *mut _;
            pc1.other = &mut pc2 as *mut _;

            (pc1, pc2)
        }
    }

    impl Drop for PeerPair {
        fn drop(&mut self) {
            if !self.other.is_null() {
                unsafe { (*self.other).other = ptr::null_mut() };
            }
        }
    }

    impl PeerConnection for PeerPair {
        type DC = Chan;

        fn on_description(&mut self, sdp: &str, sdp_type: &str) {
            if !self.other.is_null() {
                println!("Description {}: {} {}", self.id, sdp, sdp_type);
                unsafe { (*(*self.other).conn).set_remote_description(sdp, sdp_type) };
            }
        }

        fn on_candidate(&mut self, cand: &str, mid: &str) {
            if !self.other.is_null() {
                println!("Candidate {}: {} {}", self.id, cand, mid);
                unsafe { (*(*self.other).conn).add_remote_candidate(cand, mid) };
            }
        }

        fn on_state_change(&mut self, state: ConnState) {
            println!("State {}: {:?}", self.id, state);
        }

        fn on_gathering_state_change(&mut self, state: GatheringConnState) {
            println!("Gathering state {}: {:?}", self.id, state);
        }

        fn on_data_channel(&mut self, mut dc: RtcDataChannel<Chan>) {
            println!(
                "Datachannel {}: Received with label {}",
                self.id,
                dc.label()
            );
            dc.send(format!("Hello from {}", self.id).as_bytes());
        }
    };

    let (mut pp1, mut pp2) = PeerPair::new();
    let pp1_conn = &mut pp1.conn as *mut _;
    let pp2_conn = &mut pp2.conn as *mut _;

    let conf = Config::default();

    let mut rtc_pc1 = RtcPeerConnection::new(&conf, pp1, Chan(1));
    let mut rtc_pc2 = RtcPeerConnection::new(&conf, pp2, Chan(2));
    unsafe { *pp1_conn = &mut rtc_pc1 as *mut _ };
    unsafe { *pp2_conn = &mut rtc_pc2 as *mut _ };

    struct BlockingChan(usize, SyncSender<()>);

    impl DataChannel for BlockingChan {
        fn on_open(&mut self) {
            self.1.send(()).unwrap();
        }

        fn on_message(&mut self, msg: &[u8]) {
            println!("Message {}: {}", self.0, String::from_utf8_lossy(msg));
        }
    };

    let (tx, rx) = sync_channel(1);
    let mut dc1 = rtc_pc1.create_data_channel("test", BlockingChan(1, tx));
    // let mut dc1 = rtc_pc1.create_data_channel("test", Chan(1));
    rx.recv().unwrap();
    // std::thread::sleep(std::time::Duration::from_secs(2));
    dc1.send(b"Hello from 1");
}
