use std::ffi::{c_void, CStr, CString};
use std::os::raw::c_char;

use datachannel_sys as sys;

use crate::config::Config;
use crate::datachannel::{DataChannel, MakeDataChannel, RtcDataChannel};

#[derive(Debug, PartialEq)]
pub enum ConnState {
    New,
    Connecting,
    Connected,
    Disconnected,
    Failed,
    Closed,
}

impl ConnState {
    fn from_raw(state: sys::rtcState) -> Self {
        match state {
            sys::rtcState_RTC_NEW => Self::New,
            sys::rtcState_RTC_CONNECTING => Self::Connecting,
            sys::rtcState_RTC_CONNECTED => Self::Connected,
            sys::rtcState_RTC_DISCONNECTED => Self::Disconnected,
            sys::rtcState_RTC_FAILED => Self::Failed,
            sys::rtcState_RTC_CLOSED => Self::Closed,
            _ => panic!("Unknown rtcState: {}", state),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum GatheringState {
    New,
    InProgress,
    Complete,
}

impl GatheringState {
    fn from_raw(state: sys::rtcGatheringState) -> Self {
        match state {
            sys::rtcGatheringState_RTC_GATHERING_NEW => Self::New,
            sys::rtcGatheringState_RTC_GATHERING_INPROGRESS => Self::InProgress,
            sys::rtcGatheringState_RTC_GATHERING_COMPLETE => Self::Complete,
            _ => panic!("Unknown rtcGatheringState: {}", state),
        }
    }
}

#[allow(unused_variables)]
pub trait PeerConnection {
    type DC;

    fn on_description(&mut self, sdp: &str, sdp_type: &str) {}
    fn on_candidate(&mut self, cand: &str, mid: &str) {}
    fn on_conn_state_change(&mut self, state: ConnState) {}
    fn on_gathering_state_change(&mut self, state: GatheringState) {}
    fn on_data_channel(&mut self, data_channel: Box<RtcDataChannel<Self::DC>>) {}
}

pub struct RtcPeerConnection<P, D> {
    id: i32,
    pc: P,
    dc: D,
}

impl<P, D> RtcPeerConnection<P, D>
where
    P: PeerConnection + Send,
    P::DC: DataChannel + Send,
    D: MakeDataChannel<P::DC> + Send,
{
    pub fn new(config: &Config, pc: P, dc: D) -> Box<Self> {
        crate::ensure_logging();

        unsafe {
            let id = sys::rtcCreatePeerConnection(&config.as_raw());
            let mut rtc_pc = Box::new(RtcPeerConnection { id, pc, dc });
            let ptr = &mut *rtc_pc;

            sys::rtcSetUserPointer(id, ptr as *mut _ as *mut c_void);

            sys::rtcSetLocalDescriptionCallback(
                id,
                Some(RtcPeerConnection::<P, D>::local_description_cb),
            );

            sys::rtcSetLocalCandidateCallback(
                id,
                Some(RtcPeerConnection::<P, D>::local_candidate_cb),
            );

            sys::rtcSetStateChangeCallback(id, Some(RtcPeerConnection::<P, D>::state_change_cb));

            sys::rtcSetGatheringStateChangeCallback(
                id,
                Some(RtcPeerConnection::<P, D>::gathering_state_cb),
            );

            sys::rtcSetDataChannelCallback(id, Some(RtcPeerConnection::<P, D>::data_channel_cb));

            rtc_pc
        }
    }

    unsafe extern "C" fn local_description_cb(
        sdp: *const c_char,
        sdp_type: *const c_char,
        ptr: *mut c_void,
    ) {
        let rtc_pc = &mut *(ptr as *mut RtcPeerConnection<P, D>);
        let sdp = CStr::from_ptr(sdp).to_str().unwrap();
        let sdp_type = CStr::from_ptr(sdp_type).to_str().unwrap();
        rtc_pc.pc.on_description(sdp, sdp_type)
    }

    unsafe extern "C" fn local_candidate_cb(
        cand: *const c_char,
        mid: *const c_char,
        ptr: *mut c_void,
    ) {
        let rtc_pc = &mut *(ptr as *mut RtcPeerConnection<P, D>);
        let cand = CStr::from_ptr(cand).to_str().unwrap();
        let mid = CStr::from_ptr(mid).to_str().unwrap();
        rtc_pc.pc.on_candidate(cand, mid)
    }

    unsafe extern "C" fn state_change_cb(state: sys::rtcState, ptr: *mut c_void) {
        let rtc_pc = &mut *(ptr as *mut RtcPeerConnection<P, D>);
        let state = ConnState::from_raw(state);
        rtc_pc.pc.on_conn_state_change(state)
    }

    unsafe extern "C" fn gathering_state_cb(state: sys::rtcState, ptr: *mut c_void) {
        let rtc_pc = &mut *(ptr as *mut RtcPeerConnection<P, D>);
        let state = GatheringState::from_raw(state);
        rtc_pc.pc.on_gathering_state_change(state)
    }

    unsafe extern "C" fn data_channel_cb(dc: i32, ptr: *mut c_void) {
        let rtc_pc = &mut *(ptr as *mut RtcPeerConnection<P, D>);
        let dc = RtcDataChannel::new(dc, rtc_pc.dc.make());
        rtc_pc.pc.on_data_channel(dc)
    }

    pub fn create_data_channel<C>(&mut self, label: &str, handler: C) -> Box<RtcDataChannel<C>>
    where
        C: DataChannel + Send,
    {
        let label = CString::new(label).unwrap();
        let dc = unsafe { sys::rtcCreateDataChannel(self.id, label.as_ptr()) };
        RtcDataChannel::new(dc, handler)
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

impl<P, D> Drop for RtcPeerConnection<P, D> {
    fn drop(&mut self) {
        unsafe { sys::rtcDeletePeerConnection(self.id) };
    }
}
