use std::collections::HashSet;
use std::env;
use std::thread;
use std::time::Duration;

use crossbeam_channel::{select, unbounded, Sender};
use datachannel::{
    Config, ConnectionState, DataChannel, GatheringState, IceCandidate, MakeDataChannel,
    PeerConnection, RtcDataChannel, RtcPeerConnection, SessionDescription,
};

enum PeerMsg {
    RemoteDescription { sess_desc: SessionDescription },
    RemoteCandidate { cand: IceCandidate },
    Stop,
}

struct Chan {
    id: usize,
    output: Sender<String>,
    ready: Option<Sender<()>>,
}

impl Chan {
    fn new(id: usize, output: Sender<String>, ready: Option<Sender<()>>) -> Self {
        Chan { id, output, ready }
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
        let msg = String::from_utf8_lossy(msg).to_string();
        println!("Message {}: {}", self.id, &msg);
        self.output.send(msg).unwrap();
    }
}

impl MakeDataChannel<Chan> for Chan {
    fn make(&self) -> Chan {
        let ready = match &self.ready {
            None => None,
            Some(ready) => Some(ready.clone()),
        };

        Chan {
            id: self.id,
            output: self.output.clone(),
            ready,
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

    fn on_description(&mut self, sess_desc: SessionDescription) {
        println!("Description {}: {:?}", self.id, &sess_desc);
        self.signaling
            .send(PeerMsg::RemoteDescription { sess_desc })
            .unwrap();
    }

    fn on_candidate(&mut self, cand: IceCandidate) {
        println!("Candidate {}: {} {}", self.id, &cand.candidate, &cand.mid);
        self.signaling
            .send(PeerMsg::RemoteCandidate { cand })
            .unwrap();
    }

    fn on_conn_state_change(&mut self, state: ConnectionState) {
        println!("State {}: {:?}", self.id, state);
    }

    fn on_gathering_state_change(&mut self, state: GatheringState) {
        println!("Gathering state {}: {:?}", self.id, state);
    }

    fn on_data_channel(&mut self, mut dc: Box<RtcDataChannel<Chan>>) {
        println!(
            "Datachannel {}: Received with label {}",
            self.id,
            dc.label()
        );
        dc.send(format!("Hello from {}", self.id).as_bytes())
            .unwrap();
        self.dc.replace(dc);
    }
}

#[test]
fn test_connectivity() {
    env::set_var("RUST_LOG", "info");
    let _ = env_logger::try_init();

    let id1 = 1;
    let id2 = 2;

    let (tx_res, rx_res) = unbounded();
    let (tx_peer1, rx_peer1) = unbounded();
    let (tx_peer2, rx_peer2) = unbounded();

    let conn1 = LocalConn::new(id1, tx_peer2.clone());
    let conn2 = LocalConn::new(id2, tx_peer1.clone());

    let chan1 = Chan::new(id1, tx_res.clone(), None);
    let chan2 = Chan::new(id2, tx_res.clone(), None);

    let ice_servers = vec!["stun:stun.l.google.com:19302".to_string()];
    let conf = Config::new(ice_servers);

    let mut pc1 = RtcPeerConnection::new(&conf, conn1, chan1).unwrap();
    let mut pc2 = RtcPeerConnection::new(&conf, conn2, chan2).unwrap();

    let t2 = thread::spawn(move || {
        while let Ok(msg) = rx_peer2.recv() {
            match msg {
                PeerMsg::RemoteDescription { sess_desc } => {
                    pc2.set_remote_description(&sess_desc).unwrap();
                }
                PeerMsg::RemoteCandidate { cand } => {
                    pc2.add_remote_candidate(&cand).unwrap();
                }
                PeerMsg::Stop => return,
            }
        }
    });

    let t1 = thread::spawn(move || {
        let (tx_ready, rx_ready) = unbounded();
        let mut dc = pc1
            .create_data_channel("test", Chan::new(id1, tx_res.clone(), Some(tx_ready)))
            .unwrap();

        loop {
            select! {
                recv(rx_ready) -> _ =>
                    dc.send(format!("Hello from {}", id1).as_bytes()).unwrap(),
                recv(rx_peer1) -> msg => {
                    match msg.unwrap() {
                        PeerMsg::RemoteDescription { sess_desc } => {
                            pc1.set_remote_description(&sess_desc).unwrap();
                        }
                        PeerMsg::RemoteCandidate { cand } => {
                            pc1.add_remote_candidate(&cand).unwrap();
                        },
                        PeerMsg::Stop => return,
                    }
                }
            }
        }
    });

    let mut expected = HashSet::new();
    expected.insert("Hello from 1".to_string());
    expected.insert("Hello from 2".to_string());

    let mut res = HashSet::new();
    res.insert(rx_res.recv_timeout(Duration::from_secs(3)).unwrap());
    res.insert(rx_res.recv_timeout(Duration::from_secs(3)).unwrap());

    assert_eq!(expected, res);

    tx_peer1.send(PeerMsg::Stop).unwrap();
    tx_peer2.send(PeerMsg::Stop).unwrap();

    t2.join().unwrap();
    t1.join().unwrap();
}
