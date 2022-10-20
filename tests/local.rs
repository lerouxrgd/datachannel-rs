use std::collections::HashSet;
use std::thread;
use std::time::Duration;

use crossbeam_channel::{self as chan, select};

use datachannel::{
    ConnectionState, DataChannelHandler, DataChannelInfo, GatheringState, IceCandidate,
    PeerConnectionHandler, RtcConfig, RtcDataChannel, RtcPeerConnection, SessionDescription,
};

#[cfg(feature = "log")]
use log as logger;
#[cfg(feature = "tracing")]
use tracing as logger;

enum ConnectionMsg {
    RemoteDescription { sess_desc: SessionDescription },
    RemoteCandidate { cand: IceCandidate },
    Stop,
}

struct Ping {
    output: chan::Sender<String>,
    ready: chan::Sender<()>,
}

impl Ping {
    fn new(output: chan::Sender<String>, ready: chan::Sender<()>) -> Self {
        Ping { output, ready }
    }
}

impl DataChannelHandler for Ping {
    fn on_open(&mut self) {
        logger::info!("DataChannel PING: Open");
        self.ready.send(()).ok();
    }

    fn on_message(&mut self, msg: &[u8]) {
        let msg = String::from_utf8_lossy(msg).to_string();
        logger::info!("DataChannel PING: Received message: {}", &msg);
        self.output.send(msg).ok();
    }
}

#[derive(Clone)]
struct Pong {
    output: chan::Sender<String>,
}

impl Pong {
    fn new(output: chan::Sender<String>) -> Self {
        Pong { output }
    }
}

impl DataChannelHandler for Pong {
    fn on_message(&mut self, msg: &[u8]) {
        let msg = String::from_utf8_lossy(msg).to_string();
        logger::info!("DataChannel PONG: Received message: {}", &msg);
        self.output.send(msg).ok();
    }
}

struct LocalConn {
    id: usize,
    signaling: chan::Sender<ConnectionMsg>,
    pong: Pong,
    dc: Option<Box<RtcDataChannel<Pong>>>,
}

impl LocalConn {
    fn new(id: usize, pong: Pong, signaling: chan::Sender<ConnectionMsg>) -> Self {
        LocalConn {
            id,
            signaling,
            pong,
            dc: None,
        }
    }
}

impl PeerConnectionHandler for LocalConn {
    type DCH = Pong;

    fn data_channel_handler(&mut self, _info: DataChannelInfo) -> Pong {
        self.pong.clone()
    }

    fn on_description(&mut self, sess_desc: SessionDescription) {
        logger::info!("Description {}: {:?}", self.id, &sess_desc);
        self.signaling
            .send(ConnectionMsg::RemoteDescription { sess_desc })
            .ok();
    }

    fn on_candidate(&mut self, cand: IceCandidate) {
        logger::info!("Candidate {}: {} {}", self.id, &cand.candidate, &cand.mid);
        self.signaling
            .send(ConnectionMsg::RemoteCandidate { cand })
            .ok();
    }

    fn on_connection_state_change(&mut self, state: ConnectionState) {
        logger::info!("State {}: {:?}", self.id, state);
    }

    fn on_gathering_state_change(&mut self, state: GatheringState) {
        logger::info!("Gathering state {}: {:?}", self.id, state);
    }

    fn on_data_channel(&mut self, mut dc: Box<RtcDataChannel<Pong>>) {
        logger::info!(
            "PeerConnection {}: Received DataChannel with label={}, protocol={:?}, reliability={:?}",
            self.id,
            dc.label(),
            dc.protocol(),
            dc.reliability()
        );
        dc.send(format!("PONG from {}", self.id).as_bytes()).ok();
        self.dc.replace(dc);
    }
}

#[test]
fn test_connectivity() {
    #[cfg(feature = "tracing")]
    {
        tracing::subscriber::set_global_default(
            tracing_subscriber::FmtSubscriber::builder()
                .with_max_level(tracing::Level::INFO)
                .finish(),
        )
        .ok();

        datachannel::configure_logging(tracing::Level::INFO);
    }
    #[cfg(feature = "log")]
    {
        std::env::set_var("RUST_LOG", "info");
        let _ = env_logger::try_init();
    }

    let (tx_res, rx_res) = chan::unbounded::<String>();
    let (tx_peer1, rx_peer1) = chan::unbounded::<ConnectionMsg>();
    let (tx_peer2, rx_peer2) = chan::unbounded::<ConnectionMsg>();

    let id1 = 1;
    let id2 = 2;

    let pong1 = Pong::new(tx_res.clone());
    let pong2 = Pong::new(tx_res.clone());

    let conn1 = LocalConn::new(id1, pong1, tx_peer2.clone());
    let conn2 = LocalConn::new(id2, pong2, tx_peer1.clone());

    let ice_servers = vec!["stun:stun.l.google.com:19302"];
    let conf = RtcConfig::new(&ice_servers);

    let mut pc1 = RtcPeerConnection::new(&conf, conn1).unwrap();
    let mut pc2 = RtcPeerConnection::new(&conf, conn2).unwrap();

    let t2 = thread::spawn(move || {
        while let Ok(msg) = rx_peer2.recv() {
            match msg {
                ConnectionMsg::RemoteDescription { sess_desc } => {
                    pc2.set_remote_description(&sess_desc).ok();
                }
                ConnectionMsg::RemoteCandidate { cand } => {
                    pc2.add_remote_candidate(&cand).ok();
                }
                ConnectionMsg::Stop => break,
            }
        }
    });

    let t1 = thread::spawn(move || {
        let (tx_ready, rx_ready) = chan::unbounded();
        let ping = Ping::new(tx_res.clone(), tx_ready);
        let mut dc = pc1.create_data_channel("ping-pong", ping).unwrap();

        loop {
            select! {
                recv(rx_peer1) -> msg => {
                    match msg.unwrap() {
                        ConnectionMsg::RemoteDescription { sess_desc } => {
                            pc1.set_remote_description(&sess_desc).ok();
                        }
                        ConnectionMsg::RemoteCandidate { cand } => {
                            pc1.add_remote_candidate(&cand).ok();
                        },
                        ConnectionMsg::Stop => break,
                    }
                },
                recv(rx_ready) -> _ => {
                    dc.send(format!("PING from {}", id1).as_bytes()).ok();
                }
            }
        }
    });

    let mut expected = HashSet::new();
    expected.insert("PING from 1".to_string());
    expected.insert("PONG from 2".to_string());

    let mut res = HashSet::new();
    res.insert(rx_res.recv_timeout(Duration::from_secs(10)).unwrap());
    res.insert(rx_res.recv_timeout(Duration::from_secs(10)).unwrap());

    assert_eq!(expected, res);

    tx_peer1.send(ConnectionMsg::Stop).unwrap();
    tx_peer2.send(ConnectionMsg::Stop).unwrap();

    t2.join().unwrap();
    t1.join().unwrap();
}
