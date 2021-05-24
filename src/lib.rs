mod config;
mod datachannel;
mod error;
mod peerconnection;
mod track;

mod sys {
    use std::ffi::CStr;
    use std::os::raw::c_char;

    use datachannel_sys as sys;
    use lazy_static::lazy_static;

    lazy_static! {
        pub(super) static ref INIT_LOGGING: () = {
            let level = match log::max_level() {
                log::LevelFilter::Off => sys::rtcLogLevel_RTC_LOG_NONE,
                log::LevelFilter::Error => sys::rtcLogLevel_RTC_LOG_ERROR,
                log::LevelFilter::Warn => sys::rtcLogLevel_RTC_LOG_WARNING,
                log::LevelFilter::Info => sys::rtcLogLevel_RTC_LOG_INFO,
                log::LevelFilter::Debug => sys::rtcLogLevel_RTC_LOG_DEBUG,
                log::LevelFilter::Trace => sys::rtcLogLevel_RTC_LOG_VERBOSE,
            };
            unsafe { sys::rtcInitLogger(level, Some(log_callback)) };
        };
    }

    unsafe extern "C" fn log_callback(level: sys::rtcLogLevel, message: *const c_char) {
        let message = CStr::from_ptr(message).to_string_lossy();
        match level {
            sys::rtcLogLevel_RTC_LOG_NONE => (),
            sys::rtcLogLevel_RTC_LOG_ERROR => log::error!("{}", message),
            sys::rtcLogLevel_RTC_LOG_WARNING => log::warn!("{}", message),
            sys::rtcLogLevel_RTC_LOG_INFO => log::info!("{}", message),
            sys::rtcLogLevel_RTC_LOG_DEBUG => log::debug!("{}", message),
            sys::rtcLogLevel_RTC_LOG_VERBOSE => log::trace!("{}", message),
            _ => unreachable!(),
        }
    }
}

fn ensure_logging() {
    *sys::INIT_LOGGING;
}

fn ffi_string(ffi: &[u8]) -> crate::error::Result<String> {
    use std::ffi::CStr;
    let bytes = CStr::to_bytes(CStr::from_bytes_with_nul(&ffi)?);
    Ok(String::from_utf8(bytes.to_vec())?)
}

/// An optional function to preload resources, otherwise they will be loaded lazily.
pub fn preload() {
    unsafe { datachannel_sys::rtcPreload() };
}

/// An optional resources cleanup function.
pub fn cleanup() {
    unsafe { datachannel_sys::rtcCleanup() };
}

pub use crate::config::{CertificateType, RtcConfig};
pub use crate::datachannel::{DataChannelHandler, DataChannelInit, Reliability, RtcDataChannel};
pub use crate::peerconnection::{
    fmt_sdp, serde_sdp, CandidatePair, ConnectionState, GatheringState, IceCandidate,
    PeerConnectionHandler, RtcPeerConnection, SdpType, SessionDescription,
};
pub use crate::track::{Codec, Direction, RtcTrack, TrackHandler, TrackInit};

pub use webrtc_sdp as sdp;
