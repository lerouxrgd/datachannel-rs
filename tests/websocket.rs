use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_channel as chan;
use async_tungstenite::tungstenite::http::{Request, Response, StatusCode};
use async_tungstenite::tungstenite::protocol::Message;
use futures_util::{future, pin_mut, select, FutureExt, StreamExt};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[cfg(feature = "log")]
use log as logger;
#[cfg(feature = "tracing")]
use tracing as logger;

use async_tungstenite::tokio::{accept_hdr_async, connect_async};
use tokio::net::{TcpListener, TcpStream};
use tokio::spawn;
use tokio::time::timeout;

use datachannel::{
    DataChannelHandler, DataChannelInfo, DataChannelInit, IceCandidate, PeerConnectionHandler,
    Reliability, RtcConfig, RtcDataChannel, RtcPeerConnection, SdpType, SessionDescription,
};

#[derive(Debug, Serialize, Deserialize)]
struct ConnectionMsg {
    dest_id: Uuid,
    kind: MsgKind,
}

#[derive(Debug, Serialize, Deserialize)]
enum MsgKind {
    Description(SessionDescription),
    Candidate(IceCandidate),
}

// Server part

type PeerMap = Arc<Mutex<HashMap<Uuid, chan::Sender<Message>>>>;

async fn run_server() {
    let peers = PeerMap::new(Mutex::new(HashMap::new()));

    let listener = TcpListener::bind("127.0.0.1:8989")
        .await
        .expect("Listener binding failed");

    while let Ok((stream, _)) = listener.accept().await {
        spawn(handle_new_peer(peers.clone(), stream));
    }
}

async fn handle_new_peer(peers: PeerMap, stream: TcpStream) {
    let mut peer_id = None;

    let callback = |req: &Request<()>, mut resp: Response<()>| {
        let path = req.uri().path();
        let tokens = path.split('/').collect::<Vec<_>>();
        match Uuid::parse_str(tokens[1]) {
            Ok(uuid) => peer_id = Some(uuid),
            Err(err) => {
                logger::error!("Invalid uuid: {}", err);
                *resp.status_mut() = StatusCode::BAD_REQUEST;
            }
        }
        Ok(resp)
    };

    let websocket = match accept_hdr_async(stream, callback).await {
        Ok(websocket) => websocket,
        Err(err) => {
            logger::error!("WebSocket handshake failed: {}", err);
            return;
        }
    };

    let peer_id = match peer_id {
        None => return,
        Some(peer_id) => peer_id,
    };
    logger::info!("Peer {} connected", &peer_id);

    let (outgoing, mut incoming) = websocket.split();
    let (tx_ws, rx_ws) = chan::unbounded();

    peers.lock().unwrap().insert(peer_id, tx_ws);

    let reply = rx_ws.map(Ok).forward(outgoing);

    let dispatch = async {
        while let Some(Ok(msg)) = incoming.next().await {
            if !msg.is_binary() {
                continue;
            }

            let mut peer_msg = match serde_json::from_slice::<ConnectionMsg>(&msg.into_data()) {
                Ok(peer_msg) => peer_msg,
                Err(err) => {
                    logger::error!("Invalid ConnectionMsg: {}", err);
                    continue;
                }
            };
            logger::info!("Peer {} << {:?}", &peer_id, &peer_msg);

            let dest_id = peer_msg.dest_id;

            match peers.lock().unwrap().get_mut(&dest_id) {
                Some(dest_peer) => {
                    peer_msg.dest_id = peer_id;
                    logger::info!("Peer {} >> {:?}", &dest_id, &peer_msg);
                    let peer_msg = serde_json::to_vec(&peer_msg).unwrap();
                    dest_peer.try_send(Message::binary(peer_msg)).ok();
                }
                _ => logger::warn!("Peer {} not found in server", &dest_id),
            }
        }
    };

    pin_mut!(dispatch, reply);
    future::select(dispatch, reply).await;

    logger::info!("Peer {} disconnected", &peer_id);
    peers.lock().unwrap().remove(&peer_id);
}

// Client part

#[derive(Clone)]
struct DataPipe {
    output: chan::Sender<String>,
    ready: Option<chan::Sender<()>>,
}

impl DataPipe {
    fn new_sender(output: chan::Sender<String>, ready: chan::Sender<()>) -> Self {
        DataPipe {
            output,
            ready: Some(ready),
        }
    }

    fn new_receiver(output: chan::Sender<String>) -> Self {
        DataPipe {
            output,
            ready: None,
        }
    }
}

impl DataChannelHandler for DataPipe {
    fn on_open(&mut self) {
        if let Some(ready) = &mut self.ready {
            ready.try_send(()).ok();
        }
    }

    fn on_message(&mut self, msg: &[u8]) {
        let msg = String::from_utf8_lossy(msg).to_string();
        self.output.try_send(msg).ok();
    }
}

struct WsConn {
    peer_id: Uuid,
    dest_id: Uuid,
    signaling: chan::Sender<Message>,
    pipe: DataPipe,
    dc: Option<Box<RtcDataChannel<DataPipe>>>,
}

impl WsConn {
    fn new(peer_id: Uuid, dest_id: Uuid, pipe: DataPipe, signaling: chan::Sender<Message>) -> Self {
        WsConn {
            peer_id,
            dest_id,
            signaling,
            pipe,
            dc: None,
        }
    }
}

impl PeerConnectionHandler for WsConn {
    type DCH = DataPipe;

    fn data_channel_handler(&mut self, _info: DataChannelInfo) -> Self::DCH {
        self.pipe.clone()
    }

    fn on_description(&mut self, sess_desc: SessionDescription) {
        let peer_msg = ConnectionMsg {
            dest_id: self.dest_id,
            kind: MsgKind::Description(sess_desc),
        };

        self.signaling
            .try_send(Message::binary(serde_json::to_vec(&peer_msg).unwrap()))
            .ok();
    }

    fn on_candidate(&mut self, cand: IceCandidate) {
        let peer_msg = ConnectionMsg {
            dest_id: self.dest_id,
            kind: MsgKind::Candidate(cand),
        };

        self.signaling
            .try_send(Message::binary(serde_json::to_vec(&peer_msg).unwrap()))
            .ok();
    }

    fn on_data_channel(&mut self, mut dc: Box<RtcDataChannel<DataPipe>>) {
        logger::info!(
            "Received Datachannel with: label={}, protocol={:?}, reliability={:?}",
            dc.label(),
            dc.protocol(),
            dc.reliability()
        );

        dc.send(format!("Hello from {}", self.peer_id).as_bytes())
            .ok();
        self.dc.replace(dc);
    }
}

type ConnectionMap = Arc<Mutex<HashMap<Uuid, Box<RtcPeerConnection<WsConn>>>>>;
type ChannelMap = Arc<Mutex<HashMap<Uuid, Box<RtcDataChannel<DataPipe>>>>>;

async fn run_client(peer_id: Uuid, input: chan::Receiver<Uuid>, output: chan::Sender<String>) {
    let conns = ConnectionMap::new(Mutex::new(HashMap::new()));
    let chans = ChannelMap::new(Mutex::new(HashMap::new()));

    let ice_servers = vec!["stun:stun.l.google.com:19302"];
    let conf = RtcConfig::new(&ice_servers);

    let url = format!("ws://localhost:8989/{:?}", peer_id);
    let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");

    let (outgoing, mut incoming) = ws_stream.split();
    let (tx_ws, rx_ws) = chan::unbounded();

    let send = async {
        let dest_id = match input.recv().await {
            Ok(dest_id) if dest_id != peer_id => dest_id,
            Err(_) | Ok(_) => return,
        };
        logger::info!("Peer {:?} sends data", &peer_id);

        let pipe = DataPipe::new_receiver(output.clone());
        let conn = WsConn::new(peer_id, dest_id, pipe, tx_ws.clone());
        let pc = RtcPeerConnection::new(&conf, conn).unwrap();
        conns.lock().unwrap().insert(dest_id, pc);

        let (tx_ready, rx_ready) = chan::bounded(1);
        pin_mut!(rx_ready);
        let pipe = DataPipe::new_sender(output.clone(), tx_ready);

        let opts = DataChannelInit::default()
            .protocol("prototest")
            .reliability(Reliability::default().unordered());
        let mut dc = conns
            .lock()
            .unwrap()
            .get_mut(&dest_id)
            .unwrap()
            .create_data_channel_ex("sender", pipe, &opts)
            .unwrap();

        rx_ready.next().await;
        let data = format!("Hello from {:?}", peer_id);
        dc.send(data.as_bytes()).ok();

        chans.lock().unwrap().insert(dest_id, dc);
    };

    let reply = rx_ws.map(Ok).forward(outgoing);

    let receive = async {
        while let Some(Ok(msg)) = incoming.next().await {
            if !msg.is_binary() {
                continue;
            }

            let peer_msg = match serde_json::from_slice::<ConnectionMsg>(&msg.into_data()) {
                Ok(peer_msg) => peer_msg,
                Err(err) => {
                    logger::error!("Invalid ConnectionMsg: {}", err);
                    continue;
                }
            };
            let dest_id = peer_msg.dest_id;

            let mut locked = conns.lock().unwrap();
            let pc = match locked.get_mut(&dest_id) {
                Some(pc) => pc,
                None => match &peer_msg.kind {
                    MsgKind::Description(SessionDescription { sdp_type, .. })
                        if matches!(sdp_type, SdpType::Offer) =>
                    {
                        logger::info!("Client {:?} answering to {:?}", &peer_id, &dest_id);

                        let pipe = DataPipe::new_receiver(output.clone());
                        let conn = WsConn::new(peer_id, dest_id, pipe, tx_ws.clone());
                        let pc = RtcPeerConnection::new(&conf, conn).unwrap();

                        locked.insert(dest_id, pc);
                        locked.get_mut(&dest_id).unwrap()
                    }
                    _ => {
                        logger::warn!("Peer {} not found in client", &dest_id);
                        continue;
                    }
                },
            };

            match &peer_msg.kind {
                MsgKind::Description(sess_desc) => pc.set_remote_description(sess_desc).ok(),
                MsgKind::Candidate(cand) => pc.add_remote_candidate(cand).ok(),
            };
        }
    };

    let send = send.fuse();
    pin_mut!(receive, reply, send);
    loop {
        select! {
            _ = future::select(&mut receive, &mut reply) => break,
            _ = &mut send => continue,
        }
    }

    conns.lock().unwrap().clear();
    chans.lock().unwrap().clear();
}

// #[async_std::test]
#[tokio::test]
async fn test_connectivity() {
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

    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();

    spawn(run_server());

    let (tx_res, rx_res) = chan::unbounded();
    let (tx_id, rx_id) = chan::bounded(2);

    spawn(run_client(id1, rx_id.clone(), tx_res.clone()));
    spawn(run_client(id2, rx_id.clone(), tx_res.clone()));

    let mut expected = HashSet::new();
    expected.insert(format!("Hello from {:?}", id1));
    expected.insert(format!("Hello from {:?}", id2));

    tx_id.try_send(id1).unwrap();
    tx_id.try_send(id1).unwrap();

    let mut res = HashSet::new();
    let r1 = timeout(Duration::from_secs(10), rx_res.recv()).await;
    let r2 = timeout(Duration::from_secs(10), rx_res.recv()).await;
    res.insert(r1.unwrap().unwrap());
    res.insert(r2.unwrap().unwrap());

    assert_eq!(expected, res);
}
