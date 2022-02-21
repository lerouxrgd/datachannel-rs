use std::sync::Once;
use tracing::Level;

mod config;
mod datachannel;
mod error;
mod peerconnection;
mod track;

static INIT_LOGGING: Once = Once::new();

mod sys {
    use std::ffi::CStr;
    use std::os::raw::c_char;

    use datachannel_sys as sys;

    pub unsafe extern "C" fn log_callback(level: sys::rtcLogLevel, message: *const c_char) {
        let message = CStr::from_ptr(message).to_string_lossy();
        match level {
            sys::rtcLogLevel_RTC_LOG_NONE => (),
            sys::rtcLogLevel_RTC_LOG_ERROR => tracing::error!("{}", message),
            sys::rtcLogLevel_RTC_LOG_WARNING => tracing::warn!("{}", message),
            sys::rtcLogLevel_RTC_LOG_INFO => tracing::info!("{}", message),
            sys::rtcLogLevel_RTC_LOG_DEBUG => tracing::debug!("{}", message),
            sys::rtcLogLevel_RTC_LOG_VERBOSE => tracing::trace!("{}", message),
            _ => unreachable!(),
        }
    }
}

fn ffi_string(ffi: &[u8]) -> crate::error::Result<String> {
    use std::ffi::CStr;
    let bytes = CStr::to_bytes(CStr::from_bytes_with_nul(ffi)?);
    Ok(String::from_utf8(bytes.to_vec())?)
}

/// An optional function to enable libdatachannel logging, otherwise it will be disabled.
pub fn configure_logging(level: Level) {
    INIT_LOGGING.call_once(|| {
        let level = match level {
            Level::ERROR => datachannel_sys::rtcLogLevel_RTC_LOG_ERROR,
            Level::WARN => datachannel_sys::rtcLogLevel_RTC_LOG_WARNING,
            Level::INFO => datachannel_sys::rtcLogLevel_RTC_LOG_INFO,
            Level::DEBUG => datachannel_sys::rtcLogLevel_RTC_LOG_DEBUG,
            Level::TRACE => datachannel_sys::rtcLogLevel_RTC_LOG_VERBOSE,
        };

        unsafe { datachannel_sys::rtcInitLogger(level, Some(sys::log_callback)) };
    });
}

/// An optional function to preload resources, otherwise they will be loaded lazily.
pub fn preload() {
    unsafe { datachannel_sys::rtcPreload() };
}

/// An optional resources cleanup function.
pub fn cleanup() {
    unsafe { datachannel_sys::rtcCleanup() };
}

pub use crate::config::{CertificateType, RtcConfig, TransportPolicy};
pub use crate::datachannel::{DataChannelHandler, DataChannelInit, Reliability, RtcDataChannel};
pub use crate::peerconnection::{
    fmt_sdp, serde_sdp, CandidatePair, ConnectionState, GatheringState, IceCandidate,
    PeerConnectionHandler, RtcPeerConnection, SdpType, SessionDescription, SignalingState,
};
pub use crate::track::{Codec, Direction, RtcTrack, TrackHandler, TrackInit};
pub use webrtc_sdp as sdp;
