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
    pub certificate_type: CertificateType,
    pub enable_ice_tcp: bool,
    pub port_range_begin: u16,
    pub port_range_end: u16,
    pub mtu: i32,
    pub max_message_size: i32,
    pub disable_auto_negotiation: bool,
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
            certificate_type: CertificateType::Default,
            enable_ice_tcp: false,
            port_range_begin: 0,
            port_range_end: 0,
            mtu: 0,
            max_message_size: 0,
            disable_auto_negotiation: false,
        }
    }

    pub fn enable_ice_tcp(mut self) -> Self {
        self.enable_ice_tcp = true;
        self
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
            iceServers: self.ice_servers_ptrs.as_ptr() as *mut *const c_char,
            iceServersCount: self.ice_servers.len() as i32,
            certificateType: self.certificate_type as _,
            enableIceTcp: self.enable_ice_tcp,
            portRangeBegin: self.port_range_begin,
            portRangeEnd: self.port_range_end,
            mtu: self.mtu,
            maxMessageSize: self.max_message_size,
            disableAutoNegotiation: self.disable_auto_negotiation,
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
            certificate_type: self.certificate_type,
            enable_ice_tcp: self.enable_ice_tcp,
            port_range_begin: self.port_range_begin,
            port_range_end: self.port_range_end,
            mtu: self.mtu,
            max_message_size: self.max_message_size,
            disable_auto_negotiation: self.disable_auto_negotiation,
        }
    }
}

#[cfg(not(target_os = "windows"))]
#[derive(Debug, PartialEq, Clone, Copy)]
#[repr(u32)]
pub enum CertificateType {
    Default = sys::rtcCertificateType_RTC_CERTIFICATE_DEFAULT,
    ECDSA = sys::rtcCertificateType_RTC_CERTIFICATE_ECDSA,
    RSA = sys::rtcCertificateType_RTC_CERTIFICATE_RSA,
}

#[cfg(target_os = "windows")]
#[derive(Debug, PartialEq, Clone, Copy)]
#[repr(i32)]
pub enum CertificateType {
    Default = sys::rtcCertificateType_RTC_CERTIFICATE_DEFAULT,
    ECDSA = sys::rtcCertificateType_RTC_CERTIFICATE_ECDSA,
    RSA = sys::rtcCertificateType_RTC_CERTIFICATE_RSA,
}
