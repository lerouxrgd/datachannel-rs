use std::collections::HashSet;
use std::env;
use std::thread;
use std::time::Duration;

use crossbeam_channel::{self as chan, select};

use datachannel::{
    Config, ConnectionState, DataChannel, GatheringState, IceCandidate, PeerConnection,
    RtcDataChannel, RtcPeerConnection, SessionDescription,
};

enum PeerMsg {
    RemoteDescription { sess_desc: SessionDescription },
    RemoteCandidate { cand: IceCandidate },
    Stop,
}

#[derive(Clone)]
struct DataPipe {
    id: usize,
    output: chan::Sender<String>,
    ready: Option<chan::Sender<()>>,
}

impl DataPipe {
    fn new(id: usize, output: chan::Sender<String>, ready: Option<chan::Sender<()>>) -> Self {
        DataPipe { id, output, ready }
    }
}

impl DataChannel for DataPipe {
    fn on_open(&mut self) {
        log::info!("DataChannel {}: Open", self.id);
        if let Some(ready) = &self.ready {
            ready.send(()).ok();
        }
    }

    fn on_message(&mut self, msg: &[u8]) {
        let msg = String::from_utf8_lossy(msg).to_string();
        log::info!("Message {}: {}", self.id, &msg);
        self.output.send(msg).ok();
    }
}

struct LocalConn {
    id: usize,
    signaling: chan::Sender<PeerMsg>,
    dc: Option<Box<RtcDataChannel<DataPipe>>>,
}

impl LocalConn {
    fn new(id: usize, signaling: chan::Sender<PeerMsg>) -> Self {
        LocalConn {
            id,
            signaling,
            dc: None,
        }
    }
}

impl PeerConnection for LocalConn {
    type DC = DataPipe;

    fn on_description(&mut self, sess_desc: SessionDescription) {
        log::info!("Description {}: {:?}", self.id, &sess_desc);
        self.signaling
            .send(PeerMsg::RemoteDescription { sess_desc })
            .ok();
    }

    fn on_candidate(&mut self, cand: IceCandidate) {
        log::info!("Candidate {}: {} {}", self.id, &cand.candidate, &cand.mid);
        self.signaling.send(PeerMsg::RemoteCandidate { cand }).ok();
    }

    fn on_conn_state_change(&mut self, state: ConnectionState) {
        log::info!("State {}: {:?}", self.id, state);
    }

    fn on_gathering_state_change(&mut self, state: GatheringState) {
        log::info!("Gathering state {}: {:?}", self.id, state);
    }

    fn on_data_channel(&mut self, mut dc: Box<RtcDataChannel<DataPipe>>) {
        log::info!(
            "Datachannel {}: Received with: label={}, protocol={:?}, reliability={:?}",
            self.id,
            dc.label(),
            dc.protocol(),
            dc.reliability()
        );
        dc.send(format!("Hello from {}", self.id).as_bytes()).ok();
        self.dc.replace(dc);
    }
}

#[test]
fn test_connectivity() {
    env::set_var("RUST_LOG", "info");
    let _ = env_logger::try_init();

    let id1 = 1;
    let id2 = 2;

    let (tx_res, rx_res) = chan::unbounded::<String>();
    let (tx_peer1, rx_peer1) = chan::unbounded::<PeerMsg>();
    let (tx_peer2, rx_peer2) = chan::unbounded::<PeerMsg>();

    let conn1 = LocalConn::new(id1, tx_peer2.clone());
    let conn2 = LocalConn::new(id2, tx_peer1.clone());

    let pipe1 = DataPipe::new(id1, tx_res.clone(), None);
    let pipe2 = DataPipe::new(id2, tx_res.clone(), None);

    let ice_servers = vec!["stun:stun.l.google.com:19302".to_string()];
    let conf = Config::new(ice_servers);

    let mut pc1 = RtcPeerConnection::new(&conf, conn1, pipe1).unwrap();
    let mut pc2 = RtcPeerConnection::new(&conf, conn2, pipe2).unwrap();

    let t2 = thread::spawn(move || {
        while let Ok(msg) = rx_peer2.recv() {
            match msg {
                PeerMsg::RemoteDescription { sess_desc } => {
                    pc2.set_remote_description(&sess_desc).ok();
                }
                PeerMsg::RemoteCandidate { cand } => {
                    pc2.add_remote_candidate(&cand).ok();
                }
                PeerMsg::Stop => return,
            }
        }
    });

    let t1 = thread::spawn(move || {
        let (tx_ready, rx_ready) = chan::unbounded();
        let pipe = DataPipe::new(id1, tx_res.clone(), Some(tx_ready));
        let mut dc = pc1.create_data_channel("test", pipe).unwrap();

        loop {
            select! {
                recv(rx_ready) -> _ => {
                    dc.send(format!("Hello from {}", id1).as_bytes()).ok();
                },
                recv(rx_peer1) -> msg => {
                    match msg.unwrap() {
                        PeerMsg::RemoteDescription { sess_desc } => {
                            pc1.set_remote_description(&sess_desc).ok();
                        }
                        PeerMsg::RemoteCandidate { cand } => {
                            pc1.add_remote_candidate(&cand).ok();
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
    res.insert(rx_res.recv_timeout(Duration::from_secs(5)).unwrap());
    res.insert(rx_res.recv_timeout(Duration::from_secs(5)).unwrap());

    assert_eq!(expected, res);

    tx_peer1.send(PeerMsg::Stop).unwrap();
    tx_peer2.send(PeerMsg::Stop).unwrap();

    t2.join().unwrap();
    t1.join().unwrap();
}
