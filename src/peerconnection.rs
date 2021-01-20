use std::ffi::{c_void, CStr, CString};
use std::fmt;
use std::os::raw::c_char;
use std::ptr;

use datachannel_sys as sys;
use derivative::Derivative;
use parking_lot::ReentrantMutex;
use serde::{Deserialize, Serialize};
use webrtc_sdp::{parse_sdp, SdpSession};

use crate::config::RtcConfig;
use crate::datachannel::{DataChannelHandler, DataChannelInit, RtcDataChannel};
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

#[derive(Debug, PartialEq)]
pub enum SignalingState {
    Stable,
    HaveLocalOffer,
    HaveRemoteOffer,
    HaveLocalPranswer,
    HaveRemotePranswer,
}

impl SignalingState {
    fn from_raw(state: sys::rtcSignalingState) -> Self {
        match state {
            sys::rtcSignalingState_RTC_SIGNALING_STABLE => Self::Stable,
            sys::rtcSignalingState_RTC_SIGNALING_HAVE_LOCAL_OFFER => Self::HaveLocalOffer,
            sys::rtcSignalingState_RTC_SIGNALING_HAVE_REMOTE_OFFER => Self::HaveRemoteOffer,
            sys::rtcSignalingState_RTC_SIGNALING_HAVE_LOCAL_PRANSWER => Self::HaveLocalPranswer,
            sys::rtcSignalingState_RTC_SIGNALING_HAVE_REMOTE_PRANSWER => Self::HaveRemotePranswer,
            _ => panic!("Unknown rtcSignalingState: {}", state),
        }
    }
}

#[derive(Debug, PartialEq, Hash)]
pub struct CandidatePair {
    local: String,
    remote: String,
}

#[derive(Derivative, Serialize, Deserialize)]
#[derivative(Debug)]
pub struct SessionDescription {
    #[derivative(Debug(format_with = "fmt_sdp"))]
    #[serde(with = "serde_sdp")]
    pub sdp: SdpSession,
    #[serde(rename = "type")]
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

mod serde_sdp {
    use super::SdpSession;
    use serde::{de, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(sdp: &SdpSession, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&sdp.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> std::result::Result<SdpSession, D::Error>
    where
        D: Deserializer<'de>,
    {
        let sdp = String::deserialize(deserializer)?;
        webrtc_sdp::parse_sdp(&sdp, false).map_err(de::Error::custom)
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IceCandidate {
    pub candidate: String,
    #[serde(rename = "sdpMid")]
    pub mid: String,
}

#[allow(unused_variables)]
pub trait PeerConnectionHandler {
    type DC;

    fn data_channel_handler(&mut self) -> Self::DC;

    fn on_description(&mut self, sess_desc: SessionDescription) {}
    fn on_candidate(&mut self, cand: IceCandidate) {}
    fn on_connection_state_change(&mut self, state: ConnectionState) {}
    fn on_gathering_state_change(&mut self, state: GatheringState) {}
    fn on_signaling_state_change(&mut self, state: SignalingState) {}
    fn on_data_channel(&mut self, data_channel: Box<RtcDataChannel<Self::DC>>) {}
}

pub struct RtcPeerConnection<P> {
    lock: ReentrantMutex<()>,
    id: i32,
    pc_handler: P,
}

impl<P> RtcPeerConnection<P>
where
    P: PeerConnectionHandler + Send,
    P::DC: DataChannelHandler + Send,
{
    pub fn new(config: &RtcConfig, pc_handler: P) -> Result<Box<Self>> {
        crate::ensure_logging();

        unsafe {
            let id = check(sys::rtcCreatePeerConnection(&config.as_raw()))?;
            let mut rtc_pc = Box::new(RtcPeerConnection {
                lock: ReentrantMutex::new(()),
                id,
                pc_handler,
            });
            let ptr = &mut *rtc_pc;

            sys::rtcSetUserPointer(id, ptr as *mut _ as *mut c_void);

            check(sys::rtcSetLocalDescriptionCallback(
                id,
                Some(RtcPeerConnection::<P>::local_description_cb),
            ))?;

            check(sys::rtcSetLocalCandidateCallback(
                id,
                Some(RtcPeerConnection::<P>::local_candidate_cb),
            ))?;

            check(sys::rtcSetStateChangeCallback(
                id,
                Some(RtcPeerConnection::<P>::state_change_cb),
            ))?;

            check(sys::rtcSetGatheringStateChangeCallback(
                id,
                Some(RtcPeerConnection::<P>::gathering_state_cb),
            ))?;

            check(sys::rtcSetSignalingStateChangeCallback(
                id,
                Some(RtcPeerConnection::<P>::signaling_state_cb),
            ))?;

            check(sys::rtcSetDataChannelCallback(
                id,
                Some(RtcPeerConnection::<P>::data_channel_cb),
            ))?;

            Ok(rtc_pc)
        }
    }

    unsafe extern "C" fn local_description_cb(
        _: i32,
        sdp: *const c_char,
        desc_type: *const c_char,
        ptr: *mut c_void,
    ) {
        let rtc_pc = &mut *(ptr as *mut RtcPeerConnection<P>);

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

        let _guard = rtc_pc.lock.lock();
        rtc_pc.pc_handler.on_description(sess_desc);
    }

    unsafe extern "C" fn local_candidate_cb(
        _: i32,
        cand: *const c_char,
        mid: *const c_char,
        ptr: *mut c_void,
    ) {
        let rtc_pc = &mut *(ptr as *mut RtcPeerConnection<P>);

        let candidate = CStr::from_ptr(cand).to_string_lossy().to_string();
        let mid = CStr::from_ptr(mid).to_string_lossy().to_string();
        let cand = IceCandidate { candidate, mid };

        let _guard = rtc_pc.lock.lock();
        rtc_pc.pc_handler.on_candidate(cand);
    }

    unsafe extern "C" fn state_change_cb(_: i32, state: sys::rtcState, ptr: *mut c_void) {
        let rtc_pc = &mut *(ptr as *mut RtcPeerConnection<P>);

        let state = ConnectionState::from_raw(state);

        let _guard = rtc_pc.lock.lock();
        rtc_pc.pc_handler.on_connection_state_change(state);
    }

    unsafe extern "C" fn gathering_state_cb(_: i32, state: sys::rtcState, ptr: *mut c_void) {
        let rtc_pc = &mut *(ptr as *mut RtcPeerConnection<P>);

        let state = GatheringState::from_raw(state);

        let _guard = rtc_pc.lock.lock();
        rtc_pc.pc_handler.on_gathering_state_change(state);
    }

    unsafe extern "C" fn signaling_state_cb(_: i32, state: sys::rtcState, ptr: *mut c_void) {
        let rtc_pc = &mut *(ptr as *mut RtcPeerConnection<P>);

        let state = SignalingState::from_raw(state);

        let _guard = rtc_pc.lock.lock();
        rtc_pc.pc_handler.on_signaling_state_change(state);
    }

    unsafe extern "C" fn data_channel_cb(_: i32, id: i32, ptr: *mut c_void) {
        let rtc_pc = &mut *(ptr as *mut RtcPeerConnection<P>);

        let guard = rtc_pc.lock.lock();
        let dc = rtc_pc.pc_handler.data_channel_handler();
        drop(guard);

        match RtcDataChannel::new(id, dc) {
            Ok(dc) => {
                let _guard = rtc_pc.lock.lock();
                rtc_pc.pc_handler.on_data_channel(dc);
            }
            Err(err) => log::error!(
                "Couldn't create RtcDataChannel with id={} from RtcPeerConnection {:p}: {}",
                id,
                ptr,
                err
            ),
        }
    }

    pub fn add_data_channel<C>(&mut self, label: &str, dc: C) -> Result<Box<RtcDataChannel<C>>>
    where
        C: DataChannelHandler + Send,
    {
        let label = CString::new(label)?;
        let id = check(unsafe { sys::rtcAddDataChannel(self.id, label.as_ptr()) })?;
        RtcDataChannel::new(id, dc)
    }

    pub fn add_data_channel_ex<C>(
        &mut self,
        label: &str,
        dc: C,
        dc_init: &DataChannelInit,
    ) -> Result<Box<RtcDataChannel<C>>>
    where
        C: DataChannelHandler + Send,
    {
        let label = CString::new(label)?;
        let id = check(unsafe {
            sys::rtcAddDataChannelEx(self.id, label.as_ptr(), &dc_init.as_raw()?)
        })?;
        RtcDataChannel::new(id, dc)
    }

    /// Creates a boxed `[RtcDataChannel]`.
    ///
    /// This method is equivalent to calling `[add_data_channel]` and
    /// `[set_local_description]`.
    pub fn create_data_channel<C>(&mut self, label: &str, dc: C) -> Result<Box<RtcDataChannel<C>>>
    where
        C: DataChannelHandler + Send,
    {
        let label = CString::new(label)?;
        let id = check(unsafe { sys::rtcCreateDataChannel(self.id, label.as_ptr()) })?;
        RtcDataChannel::new(id, dc)
    }

    pub fn create_data_channel_ex<C>(
        &mut self,
        label: &str,
        dc: C,
        dc_init: &DataChannelInit,
    ) -> Result<Box<RtcDataChannel<C>>>
    where
        C: DataChannelHandler + Send,
    {
        let label = CString::new(label)?;
        let id = check(unsafe {
            sys::rtcCreateDataChannelEx(self.id, label.as_ptr(), &dc_init.as_raw()?)
        })?;
        RtcDataChannel::new(id, dc)
    }

    pub fn set_local_description(&mut self, desc_type: DescriptionType) -> Result<()> {
        let desc_type = CString::new(desc_type.val())?;
        check(unsafe { sys::rtcSetLocalDescription(self.id, desc_type.as_ptr()) })?;
        Ok(())
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

    pub fn local_address(&self) -> Option<String> {
        let buf_size =
            check(unsafe { sys::rtcGetLocalAddress(self.id, ptr::null_mut() as *mut c_char, 0) })
                .expect("Couldn't get buffer size") as usize;

        let mut buf = vec![0; buf_size];
        match check(unsafe {
            sys::rtcGetLocalAddress(self.id, buf.as_mut_ptr() as *mut c_char, buf_size as i32)
        }) {
            Ok(_) => match String::from_utf8(buf) {
                Ok(local) => Some(local),
                Err(err) => {
                    log::error!(
                        "Couldn't get RtcPeerConnection {:p} local_address: {}",
                        self,
                        err
                    );
                    None
                }
            },
            Err(Error::NotAvailable) => None,
            Err(err) => {
                log::warn!(
                    "Couldn't get RtcPeerConnection {:p} local_address: {}",
                    self,
                    err
                );
                None
            }
        }
    }

    pub fn remote_address(&self) -> Option<String> {
        let buf_size =
            check(unsafe { sys::rtcGetRemoteAddress(self.id, ptr::null_mut() as *mut c_char, 0) })
                .expect("Couldn't get buffer size") as usize;

        let mut buf = vec![0; buf_size];
        match check(unsafe {
            sys::rtcGetRemoteAddress(self.id, buf.as_mut_ptr() as *mut c_char, buf_size as i32)
        }) {
            Ok(_) => match String::from_utf8(buf) {
                Ok(remote) => Some(remote.trim_matches(char::from(0)).to_string()),
                Err(err) => {
                    log::error!(
                        "Couldn't get RtcPeerConnection {:p} remote_address: {}",
                        self,
                        err
                    );
                    None
                }
            },
            Err(Error::NotAvailable) => None,
            Err(err) => {
                log::warn!(
                    "Couldn't get RtcPeerConnection {:p} remote_address: {}",
                    self,
                    err
                );
                None
            }
        }
    }

    pub fn selected_candidate_pair(&self) -> Option<CandidatePair> {
        let buf_size = check(unsafe {
            sys::rtcGetSelectedCandidatePair(
                self.id,
                ptr::null_mut() as *mut c_char,
                0,
                ptr::null_mut() as *mut c_char,
                0,
            )
        })
        .expect("Couldn't get buffer size") as usize;

        let mut local_buf = vec![0; buf_size];
        let mut remote_buf = vec![0; buf_size];
        match check(unsafe {
            sys::rtcGetSelectedCandidatePair(
                self.id,
                local_buf.as_mut_ptr() as *mut c_char,
                buf_size as i32,
                remote_buf.as_mut_ptr() as *mut c_char,
                buf_size as i32,
            )
        }) {
            Ok(_) => {
                let local = crate::ffi_string(&local_buf);
                let remote = crate::ffi_string(&remote_buf);
                match (local, remote) {
                    (Ok(local), Ok(remote)) => Some(CandidatePair { local, remote }),
                    (Ok(_), Err(err)) | (Err(err), Ok(_)) | (Err(err), Err(_)) => {
                        log::error!(
                            "Couldn't get RtcPeerConnection {:p} candidate_pair: {}",
                            self,
                            err
                        );
                        None
                    }
                }
            }
            Err(Error::NotAvailable) => None,
            Err(err) => {
                log::warn!(
                    "Couldn't get RtcPeerConnection {:p} candidate_pair: {}",
                    self,
                    err
                );
                None
            }
        }
    }
}

impl<P> Drop for RtcPeerConnection<P> {
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
