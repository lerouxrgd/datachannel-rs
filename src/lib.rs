use std::ffi::{c_void, CStr, CString};
use std::marker::PhantomData;
use std::mem;
use std::os::raw::c_char;

use datachannel_sys as sys;

pub struct Config {
    pub ice_servers: Vec<*const c_char>,
    pub port_range_begin: u16,
    pub port_range_end: u16,
}

impl Default for Config {
    fn default() -> Self {
        let mut ice_servers = vec!["stun:stun.l.google.com:19302".to_string()]
            .into_iter()
            .map(|server| CString::new(server.as_str()).unwrap())
            .collect::<Vec<_>>();
        ice_servers.shrink_to_fit();
        let ice_servers: Vec<*const c_char> = ice_servers.into_iter().map(|s| s.as_ptr()).collect();
        Config {
            ice_servers,
            port_range_begin: 1024,
            port_range_end: 65535,
        }
    }
}

impl Config {
    fn as_raw(&self) -> sys::rtcConfiguration {
        sys::rtcConfiguration {
            iceServers: self.ice_servers.as_ptr() as *mut *const c_char,
            iceServersCount: self.ice_servers.len() as i32,
            portRangeBegin: self.port_range_begin,
            portRangeEnd: self.port_range_end,
        }
    }
}

#[derive(Debug, PartialEq)]
#[repr(u32)]
pub enum ConnectionState {
    New = 0,
    Connecting = 1,
    Connected = 2,
    Disconnected = 3,
    Failed = 4,
    Closed = 5,
}

impl ConnectionState {
    fn from_raw(state: sys::rtcState) -> Self {
        match state {
            0 => Self::New,
            1 => Self::Connecting,
            2 => Self::Connected,
            3 => Self::Disconnected,
            4 => Self::Failed,
            5 => Self::Closed,
            _ => panic!("Unknown rtcState: {}", state),
        }
    }
}

#[derive(Debug)]
pub enum Message<'a> {
    Str(&'a str),
    Bytes(&'a [u8]),
}

// TODO: use it in callbacks
#[derive(Debug)]
pub enum SdpType {
    Answer,
    Offer,
}

pub struct PeerConnection<F1, F2, F3, F4, F5, G1, G2, G3, G4> {
    id: i32,
    description_cb: Option<F1>,
    candidate_cb: Option<F2>,
    state_change_cb: Option<F3>,
    gathering_state_cb: Option<F4>,
    data_channel_cb: Option<F5>,
    m1: PhantomData<G1>,
    m2: PhantomData<G2>,
    m3: PhantomData<G3>,
    m4: PhantomData<G4>,
}

impl<F1, F2, F3, F4, F5, G1, G2, G3, G4> PeerConnection<F1, F2, F3, F4, F5, G1, G2, G3, G4>
where
    F1: FnMut(&str, &str),
    F2: FnMut(&str, &str),
    F3: FnMut(ConnectionState),
    F4: FnMut(ConnectionState),
    F5: FnMut(DataChannel<G1, G2, G3, G4>),
    G1: FnMut(),
    G2: FnMut(),
    G3: FnMut(&str),
    G4: FnMut(Message),
{
    pub fn new(config: &Config) -> Self {
        unsafe {
            let pc = PeerConnection {
                id: sys::rtcCreatePeerConnection(&config.as_raw()),
                description_cb: None,
                candidate_cb: None,
                state_change_cb: None,
                gathering_state_cb: None,
                data_channel_cb: None,
                m1: PhantomData,
                m2: PhantomData,
                m3: PhantomData,
                m4: PhantomData,
            };
            sys::rtcSetUserPointer(pc.id, &pc as *const _ as *mut c_void);
            pc
        }
    }

    pub fn on_local_description(&mut self, cb: F1) {
        self.description_cb = Some(cb);
        unsafe {
            sys::rtcSetLocalDescriptionCallback(
                self.id,
                Some(PeerConnection::<F1, F2, F3, F4, F5, G1, G2, G3, G4>::local_description_cb),
            )
        };
    }

    unsafe extern "C" fn local_description_cb(
        sdp: *const c_char,
        sdp_type: *const c_char,
        ptr: *mut c_void,
    ) {
        let pc: &mut PeerConnection<F1, F2, F3, F4, F5, G1, G2, G3, G4> = mem::transmute(ptr);
        let sdp = CStr::from_ptr(sdp).to_str().unwrap();
        let sdp_type = CStr::from_ptr(sdp_type).to_str().unwrap();
        (pc.description_cb.as_mut().unwrap())(sdp, sdp_type)
    }

    pub fn on_local_candidate(&mut self, cb: F2) {
        self.candidate_cb = Some(cb);
        unsafe {
            sys::rtcSetLocalCandidateCallback(
                self.id,
                Some(PeerConnection::<F1, F2, F3, F4, F5, G1, G2, G3, G4>::local_candidate_cb),
            )
        };
    }

    unsafe extern "C" fn local_candidate_cb(
        cand: *const c_char,
        mid: *const c_char,
        ptr: *mut c_void,
    ) {
        let pc: &mut PeerConnection<F1, F2, F3, F4, F5, G1, G2, G3, G4> = mem::transmute(ptr);
        let cand = CStr::from_ptr(cand).to_str().unwrap();
        let mid = CStr::from_ptr(mid).to_str().unwrap();
        (pc.candidate_cb.as_mut().unwrap())(cand, mid)
    }

    pub fn on_state_change(&mut self, cb: F3) {
        self.state_change_cb = Some(cb);
        unsafe {
            sys::rtcSetStateChangeCallback(
                self.id,
                Some(PeerConnection::<F1, F2, F3, F4, F5, G1, G2, G3, G4>::state_change_cb),
            )
        };
    }

    unsafe extern "C" fn state_change_cb(state: sys::rtcState, ptr: *mut c_void) {
        let pc: &mut PeerConnection<F1, F2, F3, F4, F5, G1, G2, G3, G4> = mem::transmute(ptr);
        let state = ConnectionState::from_raw(state);
        (pc.state_change_cb.as_mut().unwrap())(state)
    }

    pub fn on_gathering_state_change(&mut self, cb: F4) {
        self.gathering_state_cb = Some(cb);
        unsafe {
            sys::rtcSetGatheringStateChangeCallback(
                self.id,
                Some(PeerConnection::<F1, F2, F3, F4, F5, G1, G2, G3, G4>::gathering_state_cb),
            )
        };
    }

    unsafe extern "C" fn gathering_state_cb(state: sys::rtcState, ptr: *mut c_void) {
        let pc: &mut PeerConnection<F1, F2, F3, F4, F5, G1, G2, G3, G4> = mem::transmute(ptr);
        let state = ConnectionState::from_raw(state);
        (pc.gathering_state_cb.as_mut().unwrap())(state)
    }

    pub fn on_data_channel(&mut self, cb: F5) {
        self.data_channel_cb = Some(cb);
        unsafe {
            sys::rtcSetDataChannelCallback(
                self.id,
                Some(PeerConnection::<F1, F2, F3, F4, F5, G1, G2, G3, G4>::data_channel_cb),
            )
        };
    }

    unsafe extern "C" fn data_channel_cb(dc: i32, ptr: *mut c_void) {
        let pc: &mut PeerConnection<F1, F2, F3, F4, F5, G1, G2, G3, G4> = mem::transmute(ptr);
        let dc: DataChannel<G1, G2, G3, G4> = DataChannel::new(dc);
        (pc.data_channel_cb.as_mut().unwrap())(dc)
    }

    pub fn create_data_channel<D1, D2, D3, D4>(
        &mut self,
        label: &str,
    ) -> DataChannel<D1, D2, D3, D4>
    where
        D1: FnMut(),
        D2: FnMut(),
        D3: FnMut(&str),
        D4: FnMut(Message),
    {
        let label = CString::new(label).unwrap();
        let dc = unsafe { sys::rtcCreateDataChannel(self.id, label.as_ptr()) };
        DataChannel::new(dc)
    }

    pub fn set_remote_description(&mut self, sdp: &str, sdp_type: &str) {
        let sdp = CString::new(sdp).unwrap();
        let sdp_type = CString::new(sdp_type).unwrap();
        unsafe { sys::rtcSetRemoteDescription(self.id, sdp.as_ptr(), sdp_type.as_ptr()) };
    }

    pub fn add_remote_candidate(&mut self, cand: &str, mid: &str) {
        let cand = CString::new(cand).unwrap();
        let mid = CString::new(mid).unwrap();
        unsafe { sys::rtcAddRemoteCandidate(self.id, cand.as_ptr(), mid.as_ptr()) };
    }
}

impl<F1, F2, F3, F4, F5, G1, G2, G3, G4> Drop
    for PeerConnection<F1, F2, F3, F4, F5, G1, G2, G3, G4>
{
    fn drop(&mut self) {
        unsafe { sys::rtcDeletePeerConnection(self.id) };
    }
}

pub struct DataChannel<F1, F2, F3, F4> {
    id: i32,
    open_cb: Option<F1>,
    closed_cb: Option<F2>,
    error_cb: Option<F3>,
    message_cb: Option<F4>,
}

impl<F1, F2, F3, F4> DataChannel<F1, F2, F3, F4>
where
    F1: FnMut(),
    F2: FnMut(),
    F3: FnMut(&str),
    F4: FnMut(Message),
{
    fn new(id: i32) -> Self {
        let dc = DataChannel {
            id,
            open_cb: None,
            closed_cb: None,
            error_cb: None,
            message_cb: None,
        };
        unsafe { sys::rtcSetUserPointer(dc.id, &dc as *const _ as *mut c_void) };
        dc
    }

    // pub fn no_op(
    // ) -> impl FnMut(DataChannel<impl FnMut(), impl FnMut(), impl FnMut(&str), impl FnMut(Message)>)
    // {
    //     |mut dc| {
    //         dc.on_open(|| {});
    //         dc.on_closed(|| {});
    //         dc.on_error(|_err| {});
    //         dc.on_message(|_msg| {});
    //     }
    // }

    pub fn on_open(&mut self, cb: F1) {
        self.open_cb = Some(cb);
        unsafe { sys::rtcSetOpenCallback(self.id, Some(DataChannel::<F1, F2, F3, F4>::open_cb)) };
    }

    unsafe extern "C" fn open_cb(ptr: *mut c_void) {
        let dc: &mut DataChannel<F1, F2, F3, F4> = mem::transmute(ptr);
        (dc.open_cb.as_mut().unwrap())()
    }

    pub fn on_closed(&mut self, cb: F2) {
        self.closed_cb = Some(cb);
        unsafe {
            sys::rtcSetClosedCallback(self.id, Some(DataChannel::<F1, F2, F3, F4>::closed_cb))
        };
    }

    unsafe extern "C" fn closed_cb(ptr: *mut c_void) {
        let dc: &mut DataChannel<F1, F2, F3, F4> = mem::transmute(ptr);
        (dc.closed_cb.as_mut().unwrap())()
    }

    pub fn on_error(&mut self, cb: F3) {
        self.error_cb = Some(cb);
        unsafe { sys::rtcSetErrorCallback(self.id, Some(DataChannel::<F1, F2, F3, F4>::error_cb)) };
    }

    unsafe extern "C" fn error_cb(err: *const c_char, ptr: *mut c_void) {
        let dc: &mut DataChannel<F1, F2, F3, F4> = mem::transmute(ptr);
        let err = CStr::from_ptr(err).to_str().unwrap();
        (dc.error_cb.as_mut().unwrap())(err)
    }

    pub fn on_message(&mut self, cb: F4) {
        self.message_cb = Some(cb);
        unsafe {
            sys::rtcSetMessageCallback(self.id, Some(DataChannel::<F1, F2, F3, F4>::message_cb))
        };
    }

    unsafe extern "C" fn message_cb(msg: *const c_char, size: i32, ptr: *mut c_void) {
        let dc: &mut DataChannel<F1, F2, F3, F4> = mem::transmute(ptr);
        let msg = if size < 0 {
            Message::Str(CStr::from_ptr(msg).to_str().unwrap())
        } else {
            Message::Bytes(std::slice::from_raw_parts(msg as *const u8, size as usize))
        };
        (dc.message_cb.as_mut().unwrap())(msg)
    }
}

impl<F1, F2, F3, F4> Drop for DataChannel<F1, F2, F3, F4> {
    fn drop(&mut self) {
        unsafe { sys::rtcDeleteDataChannel(self.id) };
    }
}

fn wip() {
    use std::cell::RefCell;
    use std::rc::Rc;

    unsafe { sys::rtcInitLogger(5u32) };

    let conf = Config::default();

    let pc1 = Rc::new(RefCell::new(PeerConnection::new(&conf)));
    let pc2 = Rc::new(RefCell::new(PeerConnection::new(&conf)));

    let pc2_ref = Rc::clone(&pc2);
    pc1.borrow_mut().on_local_description(move |sdp, sdp_type| {
        println!("Description 1: {} {}", sdp, sdp_type);
        pc2_ref.borrow_mut().set_remote_description(sdp, sdp_type);
    });

    let pc2_ref = Rc::clone(&pc2);
    pc1.borrow_mut().on_local_candidate(move |cand, mid| {
        println!("Candidate 1: {} {}", cand, mid);
        pc2_ref.borrow_mut().add_remote_candidate(cand, mid);
    });
    pc1.borrow_mut().on_state_change(|state| {
        println!("State 1: {:?}", state);
    });
    pc1.borrow_mut().on_gathering_state_change(|state| {
        println!("Gathering state 1: {:?}", state);
    });

    // let pc1_ref = Rc::clone(&pc1);
    pc2.borrow_mut().on_local_description(move |sdp, sdp_type| {
        println!("Description 2: {} {}", sdp, sdp_type);
        // pc1_ref.borrow_mut().set_remote_description(sdp, sdp_type);
    });

    pc2.borrow_mut().on_local_candidate(|cand, mid| {
        println!("Candidate 2: {} {}", cand, mid);
        // pc1.borrow_mut().add_remote_cand
    });
    pc2.borrow_mut().on_state_change(|state| {
        println!("State 2: {:?}", state);
    });
    pc2.borrow_mut().on_gathering_state_change(|state| {
        println!("Gathering state 1: {:?}", state);
    });

    pc2.borrow_mut().on_data_channel(|mut dc| {
        dc.on_open(|| {});
        dc.on_closed(|| {});
        dc.on_error(|_err| {});
        dc.on_message(|_msg| {});
    });
    pc1.borrow_mut().on_data_channel(|mut dc| {
        dc.on_open(|| {});
        dc.on_closed(|| {});
        dc.on_error(|_err| {});
        dc.on_message(|_msg| {});
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        wip();
    }
}
