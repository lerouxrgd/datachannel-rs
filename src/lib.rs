use std::ffi::CStr;
use std::os::raw::c_char;

use datachannel_sys as sys;
use lazy_static::lazy_static;

mod config;
mod datachannel;
mod peerconnection;

pub use crate::config::Config;
pub use crate::datachannel::{DataChannel, MakeDataChannel, RtcDataChannel};
pub use crate::peerconnection::{ConnState, GatheringState, PeerConnection, RtcPeerConnection};

fn ensure_logging() {
    *INIT_LOGGING;
}

lazy_static! {
    static ref INIT_LOGGING: () = {
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
