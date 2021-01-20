use std::ffi::CString;
use std::os::raw::c_char;

use datachannel_sys as sys;
use derivative::Derivative;

#[derive(Derivative)]
#[derivative(Debug)]
pub struct RtcConfig {
    pub ice_servers: Vec<CString>,
    #[derivative(Debug = "ignore")]
    ice_servers_ptrs: Vec<*const c_char>,
    pub port_range_begin: u16,
    pub port_range_end: u16,
}

unsafe impl Send for RtcConfig {}
unsafe impl Sync for RtcConfig {}

impl RtcConfig {
    pub fn new<S: AsRef<str>>(ice_servers: &[S]) -> Self {
        let mut ice_servers = ice_servers
            .into_iter()
            .map(|server| CString::new(server.as_ref()).unwrap())
            .collect::<Vec<_>>();
        ice_servers.shrink_to_fit();
        let ice_servers_ptrs: Vec<*const c_char> = ice_servers.iter().map(|s| s.as_ptr()).collect();
        RtcConfig {
            ice_servers,
            ice_servers_ptrs,
            port_range_begin: 1024,
            port_range_end: 65535,
        }
    }

    pub fn port_range_begin(mut self, port_range_begin: u16) -> Self {
        self.port_range_begin = port_range_begin;
        self
    }

    pub fn port_range_end(mut self, port_range_end: u16) -> Self {
        self.port_range_end = port_range_end;
        self
    }

    pub(crate) fn as_raw(&self) -> sys::rtcConfiguration {
        sys::rtcConfiguration {
            iceServers: self.ice_servers.as_ptr() as *mut *const c_char,
            iceServersCount: self.ice_servers.len() as i32,
            portRangeBegin: self.port_range_begin,
            portRangeEnd: self.port_range_end,
        }
    }
}

impl Clone for RtcConfig {
    fn clone(&self) -> Self {
        let ice_servers = self.ice_servers.clone();
        let ice_servers_ptrs: Vec<*const c_char> = ice_servers.iter().map(|s| s.as_ptr()).collect();
        RtcConfig {
            ice_servers,
            ice_servers_ptrs,
            port_range_begin: self.port_range_begin,
            port_range_end: self.port_range_end,
        }
    }
}
