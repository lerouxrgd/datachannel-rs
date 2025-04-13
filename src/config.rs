use std::ffi::CString;
use std::os::raw::c_char;
use std::ptr;

use datachannel_sys as sys;
use derive_more::Debug;

#[derive(Debug)]
pub struct RtcConfig {
    pub ice_servers: Vec<CString>,
    #[debug(skip)]
    ice_servers_ptrs: Vec<*const c_char>,
    pub proxy_server: Option<CString>,
    pub bind_address: Option<CString>,
    pub certificate_type: CertificateType,
    pub ice_transport_policy: TransportPolicy,
    pub enable_ice_tcp: bool,
    pub enable_ice_udp_mux: bool,
    pub port_range_begin: u16,
    pub port_range_end: u16,
    pub mtu: i32,
    pub max_message_size: i32,
    pub disable_auto_negotiation: bool,
    pub force_media_transport: bool,
}

unsafe impl Send for RtcConfig {}
unsafe impl Sync for RtcConfig {}

impl RtcConfig {
    pub fn new<S: AsRef<str>>(ice_servers: &[S]) -> Self {
        let mut ice_servers = ice_servers
            .iter()
            .map(|server| CString::new(server.as_ref()).unwrap())
            .collect::<Vec<_>>();
        ice_servers.shrink_to_fit();
        let ice_servers_ptrs: Vec<*const c_char> = ice_servers.iter().map(|s| s.as_ptr()).collect();
        RtcConfig {
            ice_servers,
            ice_servers_ptrs,
            proxy_server: None,
            bind_address: None,
            certificate_type: CertificateType::Default,
            ice_transport_policy: TransportPolicy::All,
            enable_ice_tcp: false,
            enable_ice_udp_mux: false,
            port_range_begin: 0,
            port_range_end: 0,
            mtu: 0,
            max_message_size: 0,
            disable_auto_negotiation: false,
            force_media_transport: false,
        }
    }

    pub fn bind_address<S: AsRef<str>>(mut self, addr: &S) -> Self {
        self.bind_address = Some(CString::new(addr.as_ref()).unwrap());
        self
    }

    pub fn proxy_server<S: AsRef<str>>(mut self, server: &S) -> Self {
        self.proxy_server = Some(CString::new(server.as_ref()).unwrap());
        self
    }

    pub fn certificate_type(mut self, certificate_type: CertificateType) -> Self {
        self.certificate_type = certificate_type;
        self
    }

    pub fn enable_ice_tcp(mut self) -> Self {
        self.enable_ice_tcp = true;
        self
    }

    pub fn enable_ice_udp_mux(mut self) -> Self {
        self.enable_ice_udp_mux = true;
        self
    }

    pub fn ice_transport_policy(mut self, ice_transport_policy: TransportPolicy) -> Self {
        self.ice_transport_policy = ice_transport_policy;
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

    pub fn mtu(mut self, mtu: i32) -> Self {
        self.mtu = mtu;
        self
    }

    pub fn max_message_size(mut self, max_message_size: i32) -> Self {
        self.max_message_size = max_message_size;
        self
    }

    pub fn disable_auto_negotiation(mut self) -> Self {
        self.disable_auto_negotiation = true;
        self
    }

    pub fn force_media_transport(mut self) -> Self {
        self.force_media_transport = true;
        self
    }

    pub(crate) fn as_raw(&self) -> sys::rtcConfiguration {
        sys::rtcConfiguration {
            iceServers: self.ice_servers_ptrs.as_ptr() as *mut *const c_char,
            iceServersCount: self.ice_servers.len() as i32,
            proxyServer: self
                .proxy_server
                .as_ref()
                .map(|addr| addr.as_ptr())
                .unwrap_or(ptr::null()) as *const c_char,
            bindAddress: self
                .bind_address
                .as_ref()
                .map(|addr| addr.as_ptr())
                .unwrap_or(ptr::null()) as *const c_char,
            certificateType: self.certificate_type as _,
            iceTransportPolicy: self.ice_transport_policy as _,
            enableIceTcp: self.enable_ice_tcp,
            enableIceUdpMux: self.enable_ice_udp_mux,
            disableAutoNegotiation: self.disable_auto_negotiation,
            portRangeBegin: self.port_range_begin,
            portRangeEnd: self.port_range_end,
            mtu: self.mtu,
            maxMessageSize: self.max_message_size,
            forceMediaTransport: self.force_media_transport,
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
            proxy_server: self.proxy_server.clone(),
            bind_address: self.bind_address.clone(),
            certificate_type: self.certificate_type,
            ice_transport_policy: self.ice_transport_policy,
            enable_ice_tcp: self.enable_ice_tcp,
            enable_ice_udp_mux: self.enable_ice_udp_mux,
            disable_auto_negotiation: self.disable_auto_negotiation,
            port_range_begin: self.port_range_begin,
            port_range_end: self.port_range_end,
            mtu: self.mtu,
            max_message_size: self.max_message_size,
            force_media_transport: self.force_media_transport,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[cfg_attr(any(not(target_os = "windows"), target_env = "gnu"), repr(u32))]
#[cfg_attr(all(target_os = "windows", not(target_env = "gnu")), repr(i32))]
pub enum CertificateType {
    Default = sys::rtcCertificateType_RTC_CERTIFICATE_DEFAULT,
    ECDSA = sys::rtcCertificateType_RTC_CERTIFICATE_ECDSA,
    RSA = sys::rtcCertificateType_RTC_CERTIFICATE_RSA,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[cfg_attr(any(not(target_os = "windows"), target_env = "gnu"), repr(u32))]
#[cfg_attr(all(target_os = "windows", not(target_env = "gnu")), repr(i32))]
pub enum TransportPolicy {
    All = sys::rtcTransportPolicy_RTC_TRANSPORT_POLICY_ALL,
    Relay = sys::rtcTransportPolicy_RTC_TRANSPORT_POLICY_RELAY,
}
