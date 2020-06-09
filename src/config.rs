use std::ffi::CString;
use std::os::raw::c_char;

use datachannel_sys as sys;

pub struct Config {
    pub ice_servers: Vec<CString>,
    pub ice_servers_ptrs: Vec<*const c_char>,
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
        let ice_servers_ptrs: Vec<*const c_char> = ice_servers.iter().map(|s| s.as_ptr()).collect();
        Config {
            ice_servers,
            ice_servers_ptrs,
            port_range_begin: 1024,
            port_range_end: 65535,
        }
    }
}

impl Config {
    pub(crate) fn as_raw(&self) -> sys::rtcConfiguration {
        sys::rtcConfiguration {
            iceServers: self.ice_servers.as_ptr() as *mut *const c_char,
            iceServersCount: self.ice_servers.len() as i32,
            portRangeBegin: self.port_range_begin,
            portRangeEnd: self.port_range_end,
        }
    }
}
