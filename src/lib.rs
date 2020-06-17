mod config;
mod datachannel;
mod logs;
mod peerconnection;

pub use crate::config::Config;
pub use crate::datachannel::{DataChannel, MakeDataChannel, RtcDataChannel};
pub use crate::peerconnection::{ConnState, GatheringConnState, PeerConnection, RtcPeerConnection};

#[cfg(test)]
mod tests {
    use super::*;

    use std::env;
    use std::thread;

    use crossbeam_channel::{select, unbounded, Sender};

    enum PeerMsg {
        RemoteDescription { sdp: String, sdp_type: String },
        RemoteCandidate { cand: String, mid: String },
    }

    struct Chan {
        id: usize,
        ready: Option<Sender<()>>,
    }

    impl Chan {
        fn new(id: usize) -> Self {
            Chan { id, ready: None }
        }

        fn new_sync(id: usize, ready: Sender<()>) -> Self {
            Chan {
                id,
                ready: Some(ready),
            }
        }
    }

    impl DataChannel for Chan {
        fn on_open(&mut self) {
            println!("DataChannel {}: Open", self.id);
            if let Some(ready) = &self.ready {
                ready.send(()).unwrap();
            }
        }

        fn on_message(&mut self, msg: &[u8]) {
            println!("Message {}: {}", self.id, String::from_utf8_lossy(msg));
        }
    }

    impl MakeDataChannel<Chan> for Chan {
        fn make(&self) -> Chan {
            Chan {
                id: self.id,
                ready: None,
            }
        }
    }

    struct LocalConn {
        id: usize,
        signaling: Sender<PeerMsg>,
        dc: Option<Box<RtcDataChannel<Chan>>>,
    }

    impl LocalConn {
        fn new(id: usize, signaling: Sender<PeerMsg>) -> Self {
            LocalConn {
                id,
                signaling,
                dc: None,
            }
        }
    }

    impl PeerConnection for LocalConn {
        type DC = Chan;

        fn on_description(&mut self, sdp: &str, sdp_type: &str) {
            let (sdp, sdp_type) = (sdp.to_string(), sdp_type.to_string());
            println!("Description {}: {}\n{}", self.id, &sdp_type, &sdp);
            self.signaling
                .send(PeerMsg::RemoteDescription { sdp, sdp_type })
                .unwrap();
        }

        fn on_candidate(&mut self, cand: &str, mid: &str) {
            let (cand, mid) = (cand.to_string(), mid.to_string());
            println!("Candidate {}: {} {}", self.id, &cand, &mid);
            self.signaling
                .send(PeerMsg::RemoteCandidate { cand, mid })
                .unwrap();
        }

        fn on_state_change(&mut self, state: ConnState) {
            println!("State {}: {:?}", self.id, state);
        }

        fn on_gathering_state_change(&mut self, state: GatheringConnState) {
            println!("Gathering state {}: {:?}", self.id, state);
        }

        fn on_data_channel(&mut self, mut dc: Box<RtcDataChannel<Chan>>) {
            println!(
                "Datachannel {}: Received with label {}",
                self.id,
                dc.label()
            );
            dc.send(format!("Hello from {}", self.id).as_bytes());
            self.dc.replace(dc);
        }
    }

    #[test]
    fn test_connectivity() {
        env::set_var("RUST_LOG", "info");
        env_logger::init();

        let id1 = 1;
        let id2 = 2;

        let (tx_peer1, rx_peer1) = unbounded();
        let (tx_peer2, rx_peer2) = unbounded();

        let conn1 = LocalConn::new(id1, tx_peer2);
        let conn2 = LocalConn::new(id2, tx_peer1);

        let conf = Config::default();
        let mut pc1 = RtcPeerConnection::new(&conf, conn1, Chan::new(id1));
        let mut pc2 = RtcPeerConnection::new(&conf, conn2, Chan::new(id2));

        thread::spawn(move || {
            while let Ok(msg) = rx_peer2.recv() {
                match msg {
                    PeerMsg::RemoteDescription { sdp, sdp_type } => {
                        pc2.set_remote_description(&sdp, &sdp_type);
                    }
                    PeerMsg::RemoteCandidate { cand, mid } => {
                        pc2.add_remote_candidate(&cand, &mid);
                    }
                }
            }
        });

        thread::spawn(move || {
            let (tx_ready, rx_ready) = unbounded();
            let mut dc = pc1.create_data_channel("test", Chan::new_sync(id1, tx_ready));

            loop {
                select! {
                    recv(rx_ready) -> _ => dc.send(format!("Hello from {}", id1).as_bytes()),
                    recv(rx_peer1) -> msg => {
                        match msg.unwrap() {
                            PeerMsg::RemoteDescription { sdp, sdp_type } => {
                                pc1.set_remote_description(&sdp, &sdp_type);
                            }
                            PeerMsg::RemoteCandidate { cand, mid } => {
                                pc1.add_remote_candidate(&cand, &mid);
                            }
                        }
                    }
                }
            }
        });

        thread::sleep(std::time::Duration::from_secs(120));
    }
}
