use std::ffi::{c_void, CStr, CString};
use std::os::raw::c_char;
use std::slice;

use datachannel_sys as sys;

use crate::error::{check, Result};

#[derive(Debug, Clone, Copy)]
pub enum Direction {
    Unknown,
    SendOnly,
    RecvOnly,
    SendRecv,
    Inactive,
}

impl Direction {
    pub(crate) fn as_raw(&self) -> sys::rtcDirection {
        match self {
            Self::Unknown => sys::rtcDirection_RTC_DIRECTION_UNKNOWN,
            Self::SendOnly => sys::rtcDirection_RTC_DIRECTION_SENDONLY,
            Self::RecvOnly => sys::rtcDirection_RTC_DIRECTION_RECVONLY,
            Self::SendRecv => sys::rtcDirection_RTC_DIRECTION_SENDRECV,
            Self::Inactive => sys::rtcDirection_RTC_DIRECTION_INACTIVE,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Codec {
    H264,
    VP8,
    VP9,
    Opus,
}

impl Codec {
    pub(crate) fn as_raw(&self) -> sys::rtcCodec {
        match self {
            Self::H264 => sys::rtcCodec_RTC_CODEC_H264,
            Self::VP8 => sys::rtcCodec_RTC_CODEC_VP8,
            Self::VP9 => sys::rtcCodec_RTC_CODEC_VP9,
            Self::Opus => sys::rtcCodec_RTC_CODEC_OPUS,
        }
    }
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
}

impl TrackInit {
    pub(crate) fn as_raw(&self) -> sys::rtcTrackInit {
        sys::rtcTrackInit {
            direction: self.direction.as_raw(),
            codec: self.codec.as_raw(),
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
        }
    }
}

#[allow(unused_variables)]
pub trait TrackHandler {
    fn on_open(&mut self) {}
    fn on_closed(&mut self) {}
    fn on_error(&mut self, err: &str) {}
    fn on_message(&mut self, msg: &[u8]) {}
    fn on_buffered_amount_low(&mut self) {}
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

            check(sys::rtcSetBufferedAmountLowCallback(
                id,
                Some(RtcTrack::<T>::buffered_amount_low_cb),
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

    unsafe extern "C" fn buffered_amount_low_cb(_: i32, ptr: *mut c_void) {
        let rtc_t = &mut *(ptr as *mut RtcTrack<T>);
        rtc_t.t_handler.on_buffered_amount_low()
    }

    unsafe extern "C" fn available_cb(_: i32, ptr: *mut c_void) {
        let rtc_t = &mut *(ptr as *mut RtcTrack<T>);
        rtc_t.t_handler.on_available()
    }
}

impl<T> Drop for RtcTrack<T> {
    fn drop(&mut self) {
        match check(unsafe { sys::rtcDeleteTrack(self.id) }) {
            Err(err) => log::error!(
                "Error while dropping RtcTrack id={} {:p}: {}",
                self.id,
                self,
                err
            ),
            _ => (),
        }
    }
}
