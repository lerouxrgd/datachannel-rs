use std::ffi::{c_void, CStr, CString};
use std::fmt;
use std::os::raw::c_char;
use std::sync::atomic::{AtomicBool, Ordering};

use crossbeam_utils::Backoff;
use datachannel_sys as sys;
use derivative::Derivative;
use webrtc_sdp::{parse_sdp, SdpSession};

use crate::config::Config;
use crate::datachannel::{DataChannel, MakeDataChannel, RtcDataChannel};
use crate::error::{check, Error, Result};

#[derive(Debug, PartialEq)]
pub enum ConnectionState {
    New,
    Connecting,
    Connected,
    Disconnected,
    Failed,
    Closed,
}

impl ConnectionState {
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

#[derive(Derivative)]
#[derivative(Debug)]
pub struct SessionDescription {
    #[derivative(Debug(format_with = "fmt_sdp"))]
    pub sdp: SdpSession,
    pub desc_type: DescriptionType,
}

fn fmt_sdp(sdp: &SdpSession, f: &mut fmt::Formatter) -> std::result::Result<(), fmt::Error> {
    let sdp = sdp
        .to_string()
        .trim_end()
        .split("\r\n")
        .collect::<Vec<_>>()
        .join("; ");
    f.write_str(format!("{{ {} }}", sdp).as_str())
}

#[derive(Debug, PartialEq, Clone)]
pub enum DescriptionType {
    Answer,
    Offer,
    PrAnswer,
    Rollback,
}

impl DescriptionType {
    fn from(val: &str) -> Result<Self> {
        match val {
            "answer" => Ok(Self::Answer),
            "offer" => Ok(Self::Offer),
            "pranswer" => Ok(Self::PrAnswer),
            "rollback" => Ok(Self::Rollback),
            _ => Err(Error::InvalidArg),
        }
    }

    fn val(&self) -> &'static str {
        match self {
            Self::Answer => "answer,",
            Self::Offer => "offer,",
            Self::PrAnswer => "pranswer,",
            Self::Rollback => "rollback,",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct IceCandidate {
    pub candidate: String,
    pub mid: String,
}

#[allow(unused_variables)]
pub trait PeerConnection {
    type DC;

    fn on_description(&mut self, sess_desc: SessionDescription) {}
    fn on_candidate(&mut self, cand: IceCandidate) {}
    fn on_conn_state_change(&mut self, state: ConnectionState) {}
    fn on_gathering_state_change(&mut self, state: GatheringState) {}
    fn on_data_channel(&mut self, data_channel: Box<RtcDataChannel<Self::DC>>) {}
}

pub struct RtcPeerConnection<P, D> {
    lock: AtomicBool,
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
    pub fn new(config: &Config, pc: P, dc: D) -> Result<Box<Self>> {
        crate::ensure_logging();

        unsafe {
            let id = check(sys::rtcCreatePeerConnection(&config.as_raw()))?;
            let mut rtc_pc = Box::new(RtcPeerConnection {
                lock: AtomicBool::new(false),
                id,
                pc,
                dc,
            });
            let ptr = &mut *rtc_pc;

            sys::rtcSetUserPointer(id, ptr as *mut _ as *mut c_void);

            check(sys::rtcSetLocalDescriptionCallback(
                id,
                Some(RtcPeerConnection::<P, D>::local_description_cb),
            ))?;

            check(sys::rtcSetLocalCandidateCallback(
                id,
                Some(RtcPeerConnection::<P, D>::local_candidate_cb),
            ))?;

            check(sys::rtcSetStateChangeCallback(
                id,
                Some(RtcPeerConnection::<P, D>::state_change_cb),
            ))?;

            check(sys::rtcSetGatheringStateChangeCallback(
                id,
                Some(RtcPeerConnection::<P, D>::gathering_state_cb),
            ))?;

            check(sys::rtcSetDataChannelCallback(
                id,
                Some(RtcPeerConnection::<P, D>::data_channel_cb),
            ))?;

            Ok(rtc_pc)
        }
    }

    unsafe extern "C" fn local_description_cb(
        sdp: *const c_char,
        desc_type: *const c_char,
        ptr: *mut c_void,
    ) {
        let rtc_pc = &mut *(ptr as *mut RtcPeerConnection<P, D>);

        let sdp = CStr::from_ptr(sdp).to_string_lossy();
        let sdp = match parse_sdp(&sdp, false) {
            Ok(sdp) => sdp,
            Err(err) => {
                log::warn!("Ignoring invalid SDP: {}", err);
                log::debug!("{}", sdp);
                return;
            }
        };

        let desc_type = CStr::from_ptr(desc_type).to_string_lossy();
        let desc_type = match DescriptionType::from(&desc_type) {
            Ok(desc_type) => desc_type,
            Err(_) => {
                log::warn!(
                    "Ignoring session with invalid DescriptionType: {}",
                    desc_type
                );
                log::debug!("{}", sdp);
                return;
            }
        };

        let sess_desc = SessionDescription { sdp, desc_type };

        let backoff = Backoff::new();
        while rtc_pc.lock.compare_and_swap(false, true, Ordering::Acquire) {
            backoff.snooze();
        }
        rtc_pc.pc.on_description(sess_desc);
        rtc_pc.lock.store(false, Ordering::Release);
    }

    unsafe extern "C" fn local_candidate_cb(
        cand: *const c_char,
        mid: *const c_char,
        ptr: *mut c_void,
    ) {
        let rtc_pc = &mut *(ptr as *mut RtcPeerConnection<P, D>);
        let candidate = CStr::from_ptr(cand).to_string_lossy().to_string();
        let mid = CStr::from_ptr(mid).to_string_lossy().to_string();
        let cand = IceCandidate { candidate, mid };

        let backoff = Backoff::new();
        while rtc_pc.lock.compare_and_swap(false, true, Ordering::Acquire) {
            backoff.snooze();
        }
        rtc_pc.pc.on_candidate(cand);
        rtc_pc.lock.store(false, Ordering::Release);
    }

    unsafe extern "C" fn state_change_cb(state: sys::rtcState, ptr: *mut c_void) {
        let rtc_pc = &mut *(ptr as *mut RtcPeerConnection<P, D>);
        let state = ConnectionState::from_raw(state);

        let backoff = Backoff::new();
        while rtc_pc.lock.compare_and_swap(false, true, Ordering::Acquire) {
            backoff.snooze();
        }
        rtc_pc.pc.on_conn_state_change(state);
        rtc_pc.lock.store(false, Ordering::Release);
    }

    unsafe extern "C" fn gathering_state_cb(state: sys::rtcState, ptr: *mut c_void) {
        let rtc_pc = &mut *(ptr as *mut RtcPeerConnection<P, D>);
        let state = GatheringState::from_raw(state);

        let backoff = Backoff::new();
        while rtc_pc.lock.compare_and_swap(false, true, Ordering::Acquire) {
            backoff.snooze();
        }
        rtc_pc.pc.on_gathering_state_change(state);
        rtc_pc.lock.store(false, Ordering::Release);
    }

    unsafe extern "C" fn data_channel_cb(dc: i32, ptr: *mut c_void) {
        let rtc_pc = &mut *(ptr as *mut RtcPeerConnection<P, D>);
        match RtcDataChannel::new(dc, rtc_pc.dc.make()) {
            Ok(dc) => {
                let backoff = Backoff::new();
                while rtc_pc.lock.compare_and_swap(false, true, Ordering::Acquire) {
                    backoff.snooze();
                }
                rtc_pc.pc.on_data_channel(dc);
                rtc_pc.lock.store(false, Ordering::Release);
            }
            Err(err) => log::error!(
                "Couldn't create RtcDataChannel with id={} from RtcPeerConnection {:p}: {}",
                dc,
                ptr,
                err
            ),
        }
    }

    pub fn create_data_channel<C>(&mut self, label: &str, dc: C) -> Result<Box<RtcDataChannel<C>>>
    where
        C: DataChannel + Send,
    {
        let label = CString::new(label)?;
        let id = check(unsafe { sys::rtcCreateDataChannel(self.id, label.as_ptr()) })?;
        RtcDataChannel::new(id, dc)
    }

    pub fn set_remote_description(&mut self, sess_desc: &SessionDescription) -> Result<()> {
        let sdp = CString::new(sess_desc.sdp.to_string())?;
        let desc_type = CString::new(sess_desc.desc_type.val())?;
        check(unsafe { sys::rtcSetRemoteDescription(self.id, sdp.as_ptr(), desc_type.as_ptr()) })?;
        Ok(())
    }

    pub fn add_remote_candidate(&mut self, cand: &IceCandidate) -> Result<()> {
        let mid = CString::new(cand.mid.clone())?;
        let cand = CString::new(cand.candidate.clone())?;
        unsafe { sys::rtcAddRemoteCandidate(self.id, cand.as_ptr(), mid.as_ptr()) };
        Ok(())
    }
}

impl<P, D> Drop for RtcPeerConnection<P, D> {
    fn drop(&mut self) {
        match check(unsafe { sys::rtcDeletePeerConnection(self.id) }) {
            Err(err) => log::error!(
                "Error while dropping RtcPeerConnection id={} {:p}: {}",
                self.id,
                self,
                err
            ),
            _ => (),
        }
    }
}
