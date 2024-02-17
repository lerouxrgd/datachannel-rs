use std::ffi::{c_void, CStr, CString};
use std::os::raw::c_char;
use std::{ptr, slice};

use datachannel_sys as sys;
use webrtc_sdp::media_type::{parse_media_vector, SdpMedia};
use webrtc_sdp::{parse_sdp_line, SdpLine};

use crate::error::{check, Result};
use crate::logger;

#[derive(Debug, Clone, Copy)]
#[cfg_attr(any(not(target_os = "windows"), target_env = "gnu"), repr(u32))]
#[cfg_attr(all(target_os = "windows", not(target_env = "gnu")), repr(i32))]
pub enum Direction {
    Unknown = sys::rtcDirection_RTC_DIRECTION_UNKNOWN,
    SendOnly = sys::rtcDirection_RTC_DIRECTION_SENDONLY,
    RecvOnly = sys::rtcDirection_RTC_DIRECTION_RECVONLY,
    SendRecv = sys::rtcDirection_RTC_DIRECTION_SENDRECV,
    Inactive = sys::rtcDirection_RTC_DIRECTION_INACTIVE,
}

#[cfg(any(not(target_os = "windows"), target_env = "gnu"))]
impl TryFrom<u32> for Direction {
    type Error = ();

    fn try_from(v: u32) -> std::result::Result<Self, Self::Error> {
        match v {
            x if x == Self::Unknown as u32 => Ok(Self::Unknown),
            x if x == Self::SendOnly as u32 => Ok(Self::SendOnly),
            x if x == Self::RecvOnly as u32 => Ok(Self::RecvOnly),
            x if x == Self::SendRecv as u32 => Ok(Self::SendRecv),
            x if x == Self::Inactive as u32 => Ok(Self::Inactive),
            _ => Err(()),
        }
    }
}

#[cfg(all(target_os = "windows", not(target_env = "gnu")))]
impl TryFrom<i32> for Direction {
    type Error = ();

    fn try_from(v: i32) -> std::result::Result<Self, Self::Error> {
        match v {
            x if x == Self::Unknown as i32 => Ok(Self::Unknown),
            x if x == Self::SendOnly as i32 => Ok(Self::SendOnly),
            x if x == Self::RecvOnly as i32 => Ok(Self::RecvOnly),
            x if x == Self::SendRecv as i32 => Ok(Self::SendRecv),
            x if x == Self::Inactive as i32 => Ok(Self::Inactive),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(any(not(target_os = "windows"), target_env = "gnu"), repr(u32))]
#[cfg_attr(all(target_os = "windows", not(target_env = "gnu")), repr(i32))]
pub enum Codec {
    H264 = sys::rtcCodec_RTC_CODEC_H264,
    VP8 = sys::rtcCodec_RTC_CODEC_VP8,
    VP9 = sys::rtcCodec_RTC_CODEC_VP9,
    Opus = sys::rtcCodec_RTC_CODEC_OPUS,
}

#[derive(Debug, Clone)]
pub struct TrackInit {
    pub direction: Direction,
    pub codec: Codec,
    pub payload_type: i32,
    pub ssrc: u32,
    pub mid: CString,
    pub name: Option<CString>,
    pub msid: Option<CString>,
    pub track_id: Option<CString>,
    pub profile: Option<CString>,
}

impl TrackInit {
    pub(crate) fn as_raw(&self) -> sys::rtcTrackInit {
        sys::rtcTrackInit {
            direction: self.direction as _,
            codec: self.codec as _,
            payloadType: self.payload_type,
            ssrc: self.ssrc,
            mid: self.mid.as_ptr(),
            name: self
                .name
                .as_ref()
                .map(|s| s.as_ptr())
                .unwrap_or(std::ptr::null()),
            msid: self
                .msid
                .as_ref()
                .map(|s| s.as_ptr())
                .unwrap_or(std::ptr::null()),
            trackId: self
                .track_id
                .as_ref()
                .map(|s| s.as_ptr())
                .unwrap_or(std::ptr::null()),
            profile: self
                .profile
                .as_ref()
                .map(|s| s.as_ptr())
                .unwrap_or(std::ptr::null()),
        }
    }
}

#[allow(unused_variables)]
pub trait TrackHandler {
    fn on_open(&mut self) {}
    fn on_closed(&mut self) {}
    fn on_error(&mut self, err: &str) {}
    fn on_message(&mut self, msg: &[u8]) {}
    fn on_available(&mut self) {}
}

pub struct RtcTrack<T> {
    id: i32,
    t_handler: T,
}

impl<T> RtcTrack<T>
where
    T: TrackHandler + Send,
{
    pub(crate) fn new(id: i32, t_handler: T) -> Result<Box<Self>> {
        unsafe {
            let mut rtc_t = Box::new(RtcTrack { id, t_handler });
            let ptr = &mut *rtc_t;

            sys::rtcSetUserPointer(id, ptr as *mut _ as *mut c_void);

            check(sys::rtcSetOpenCallback(id, Some(RtcTrack::<T>::open_cb)))?;

            check(sys::rtcSetClosedCallback(
                id,
                Some(RtcTrack::<T>::closed_cb),
            ))?;

            check(sys::rtcSetErrorCallback(id, Some(RtcTrack::<T>::error_cb)))?;

            check(sys::rtcSetMessageCallback(
                id,
                Some(RtcTrack::<T>::message_cb),
            ))?;

            check(sys::rtcSetAvailableCallback(
                id,
                Some(RtcTrack::<T>::available_cb),
            ))?;

            Ok(rtc_t)
        }
    }

    unsafe extern "C" fn open_cb(_: i32, ptr: *mut c_void) {
        let rtc_t = &mut *(ptr as *mut RtcTrack<T>);
        rtc_t.t_handler.on_open()
    }

    unsafe extern "C" fn closed_cb(_: i32, ptr: *mut c_void) {
        let rtc_t = &mut *(ptr as *mut RtcTrack<T>);
        rtc_t.t_handler.on_closed()
    }

    unsafe extern "C" fn error_cb(_: i32, err: *const c_char, ptr: *mut c_void) {
        let rtc_t = &mut *(ptr as *mut RtcTrack<T>);
        let err = CStr::from_ptr(err).to_string_lossy();
        rtc_t.t_handler.on_error(&err)
    }

    unsafe extern "C" fn message_cb(_: i32, msg: *const c_char, size: i32, ptr: *mut c_void) {
        let rtc_t = &mut *(ptr as *mut RtcTrack<T>);
        let msg = if size < 0 {
            CStr::from_ptr(msg).to_bytes()
        } else {
            slice::from_raw_parts(msg as *const u8, size as usize)
        };
        rtc_t.t_handler.on_message(msg)
    }

    unsafe extern "C" fn available_cb(_: i32, ptr: *mut c_void) {
        let rtc_t = &mut *(ptr as *mut RtcTrack<T>);
        rtc_t.t_handler.on_available()
    }

    pub fn send(&mut self, msg: &[u8]) -> Result<()> {
        check(unsafe {
            sys::rtcSendMessage(self.id, msg.as_ptr() as *const c_char, msg.len() as i32)
        })
        .map(|_| ())
    }

    pub fn description(&self) -> Option<Vec<SdpMedia>> {
        let buf_size = check(unsafe {
            sys::rtcGetTrackDescription(self.id, ptr::null_mut() as *mut c_char, 0)
        })
        .expect("Couldn't get buffer size") as usize;

        let mut buf = vec![0; buf_size];
        check(unsafe {
            sys::rtcGetTrackDescription(self.id, buf.as_mut_ptr() as *mut c_char, buf_size as i32)
        })
        .map_err(|err| {
            logger::warn!(
                "Couldn't get description for RtcTrack id={} {:p}, {}",
                self.id,
                self,
                err
            );
        })
        .ok()
        .and_then(|_| {
            crate::ffi_string(&buf)
                .map_err(|err| {
                    logger::error!(
                        "Couldn't get description for RtcTrack id={} {:p}, {}",
                        self.id,
                        self,
                        err
                    );
                })
                .ok()
        })
        .and_then(|description| {
            description
                .split('\n')
                .enumerate()
                .map(|(line_number, line)| parse_sdp_line(line, line_number))
                .collect::<std::result::Result<Vec<SdpLine>, _>>()
                .map_err(|err| logger::error!("Couldn't parse SdpLine: {}", err))
                .ok()
        })
        .and_then(|mut sdp_lines| {
            parse_media_vector(&mut sdp_lines)
                .map_err(|err| logger::error!("Couldn't parse SdpMedia: {}", err))
                .ok()
        })
    }

    pub fn mid(&self) -> String {
        let buf_size =
            check(unsafe { sys::rtcGetTrackMid(self.id, ptr::null_mut() as *mut c_char, 0) })
                .expect("Couldn't get buffer size") as usize;

        let mut buf = vec![0; buf_size];
        check(unsafe {
            sys::rtcGetTrackMid(self.id, buf.as_mut_ptr() as *mut c_char, buf_size as i32)
        })
        .map_err(|err| {
            logger::warn!(
                "Couldn't get mid for RtcTrack id={} {:p}, {}",
                self.id,
                self,
                err
            );
        })
        .ok()
        .and_then(|_| {
            crate::ffi_string(&buf)
                .map_err(|err| {
                    logger::error!(
                        "Couldn't get mid for RtcTrack id={} {:p}, {}",
                        self.id,
                        self,
                        err
                    );
                })
                .ok()
        })
        .unwrap_or_default()
    }

    pub fn direction(&self) -> Direction {
        let mut direction = sys::rtcDirection_RTC_DIRECTION_UNKNOWN;
        check(unsafe { sys::rtcGetTrackDirection(self.id, &mut direction) })
            .expect("Couldn't get RtcTrack direction");
        Direction::try_from(direction).unwrap_or(Direction::Unknown)
    }
}

impl<T> Drop for RtcTrack<T> {
    fn drop(&mut self) {
        if let Err(err) = check(unsafe { sys::rtcDeleteTrack(self.id) }) {
            logger::error!(
                "Error while dropping RtcTrack id={} {:p}: {}",
                self.id,
                self,
                err
            );
        }
    }
}
