use std::convert::TryFrom;
use std::ffi::{c_void, CStr};
use std::os::raw::c_char;
use std::slice;

use datachannel_sys as sys;

use crate::error::{check, Error, Result};

#[derive(Debug, Default)]
pub struct Reliability {
    pub unordered: bool,
    pub unreliable: bool,
    pub max_packet_life_time: u32,
    pub max_retransmits: u32,
}

impl Reliability {
    fn from_raw(raw: sys::rtcReliability) -> Self {
        Self {
            unordered: raw.unordered,
            unreliable: raw.unreliable,
            max_packet_life_time: raw.maxPacketLifeTime,
            max_retransmits: raw.maxRetransmits,
        }
    }

    pub fn unordered(mut self) -> Self {
        self.unordered = true;
        self
    }

    pub fn unreliable(mut self) -> Self {
        self.unreliable = true;
        self
    }

    pub fn max_packet_life_time(mut self, max_packet_life_time: u32) -> Self {
        self.max_packet_life_time = max_packet_life_time;
        self
    }

    pub fn max_retransmits(mut self, max_retransmits: u32) -> Self {
        self.max_retransmits = max_retransmits;
        self
    }

    pub(crate) fn as_raw(&self) -> sys::rtcReliability {
        sys::rtcReliability {
            unordered: self.unordered,
            unreliable: self.unreliable,
            maxPacketLifeTime: self.max_packet_life_time,
            maxRetransmits: self.max_retransmits,
        }
    }
}

pub trait MakeDataChannel<D>
where
    D: DataChannel + Send,
{
    fn make(&mut self) -> D;
}

impl<D> MakeDataChannel<D> for D
where
    D: DataChannel + Send + Clone,
{
    fn make(&mut self) -> D {
        self.clone()
    }
}

#[allow(unused_variables)]
pub trait DataChannel {
    fn on_open(&mut self) {}
    fn on_closed(&mut self) {}
    fn on_error(&mut self, err: &str) {}
    fn on_message(&mut self, msg: &[u8]) {}
    fn on_buffered_amount_low(&mut self) {}
}

pub struct RtcDataChannel<D> {
    id: i32,
    dc: D,
}

impl<D> RtcDataChannel<D>
where
    D: DataChannel + Send,
{
    pub(crate) fn new(id: i32, dc: D) -> Result<Box<Self>> {
        unsafe {
            let mut rtc_dc = Box::new(RtcDataChannel { id, dc });
            let ptr = &mut *rtc_dc;

            sys::rtcSetUserPointer(id, ptr as *mut _ as *mut c_void);

            check(sys::rtcSetOpenCallback(
                id,
                Some(RtcDataChannel::<D>::open_cb),
            ))?;

            check(sys::rtcSetClosedCallback(
                id,
                Some(RtcDataChannel::<D>::closed_cb),
            ))?;

            check(sys::rtcSetErrorCallback(
                id,
                Some(RtcDataChannel::<D>::error_cb),
            ))?;

            check(sys::rtcSetMessageCallback(
                id,
                Some(RtcDataChannel::<D>::message_cb),
            ))?;

            check(sys::rtcSetBufferedAmountLowCallback(
                id,
                Some(RtcDataChannel::<D>::buffered_amount_low_cb),
            ))?;

            Ok(rtc_dc)
        }
    }

    unsafe extern "C" fn open_cb(_: i32, ptr: *mut c_void) {
        let rtc_dc = &mut *(ptr as *mut RtcDataChannel<D>);
        rtc_dc.dc.on_open()
    }

    unsafe extern "C" fn closed_cb(_: i32, ptr: *mut c_void) {
        let rtc_dc = &mut *(ptr as *mut RtcDataChannel<D>);
        rtc_dc.dc.on_closed()
    }

    unsafe extern "C" fn error_cb(_: i32, err: *const c_char, ptr: *mut c_void) {
        let rtc_dc = &mut *(ptr as *mut RtcDataChannel<D>);
        let err = CStr::from_ptr(err).to_string_lossy();
        rtc_dc.dc.on_error(&err)
    }

    unsafe extern "C" fn message_cb(_: i32, msg: *const c_char, size: i32, ptr: *mut c_void) {
        let rtc_dc = &mut *(ptr as *mut RtcDataChannel<D>);
        let msg = if size < 0 {
            CStr::from_ptr(msg).to_bytes()
        } else {
            slice::from_raw_parts(msg as *const u8, size as usize)
        };
        rtc_dc.dc.on_message(msg)
    }

    unsafe extern "C" fn buffered_amount_low_cb(_: i32, ptr: *mut c_void) {
        let rtc_dc = &mut *(ptr as *mut RtcDataChannel<D>);
        rtc_dc.dc.on_buffered_amount_low()
    }

    pub fn send(&mut self, msg: &[u8]) -> Result<()> {
        check(unsafe {
            sys::rtcSendMessage(self.id, msg.as_ptr() as *const c_char, msg.len() as i32)
        })
        .map(|_| ())
    }

    pub fn label(&self) -> String {
        const BUF_SIZE: usize = 256;
        let mut buf = vec![0; BUF_SIZE];
        match check(unsafe {
            sys::rtcGetDataChannelLabel(self.id, buf.as_mut_ptr() as *mut c_char, BUF_SIZE as i32)
        }) {
            Ok(_) => match String::from_utf8(buf) {
                Ok(label) => label.trim_matches(char::from(0)).to_string(),
                Err(err) => {
                    log::error!(
                        "Couldn't get label for RtcDataChannel id={} {:p}, {}",
                        self.id,
                        self,
                        err
                    );
                    String::default()
                }
            },
            Err(err) => {
                log::error!(
                    "Couldn't get label for RtcDataChannel id={} {:p}, {}",
                    self.id,
                    self,
                    err
                );
                String::default()
            }
        }
    }

    pub fn protocol(&self) -> Option<String> {
        const BUF_SIZE: usize = 256;
        let mut buf = vec![0; BUF_SIZE];
        match check(unsafe {
            sys::rtcGetDataChannelProtocol(
                self.id,
                buf.as_mut_ptr() as *mut c_char,
                BUF_SIZE as i32,
            )
        }) {
            Ok(1) => None,
            Ok(_) => match String::from_utf8(buf) {
                Ok(protocol) => Some(protocol.trim_matches(char::from(0)).to_string()),
                Err(err) => {
                    log::error!(
                        "Couldn't get protocol for RtcDataChannel id={} {:p}, {}",
                        self.id,
                        self,
                        err
                    );
                    Some(String::default())
                }
            },
            Err(err) => {
                log::error!(
                    "Couldn't get protocol for RtcDataChannel id={} {:p}, {}",
                    self.id,
                    self,
                    err
                );
                Some(String::default())
            }
        }
    }

    pub fn reliability(&self) -> Reliability {
        let mut reliability = sys::rtcReliability {
            unordered: false,
            unreliable: false,
            maxPacketLifeTime: 0,
            maxRetransmits: 0,
        };

        check(unsafe { sys::rtcGetDataChannelReliability(self.id, &mut reliability) })
            .expect("Couldn't get RtcDataChannel reliability");

        Reliability::from_raw(reliability)
    }

    pub fn buffered_amount(&self) -> usize {
        match check(unsafe { sys::rtcGetBufferedAmount(self.id) }) {
            Ok(amount) => amount as usize,
            Err(err) => {
                log::error!(
                    "Couldn't get buffered_amount for RtcDataChannel id={} {:p}, {}",
                    self.id,
                    self,
                    err
                );
                0
            }
        }
    }

    pub fn set_buffered_amount_low_threshold(&mut self, amount: usize) -> Result<()> {
        let amount = i32::try_from(amount).map_err(|_| Error::InvalidArg)?;
        check(unsafe { sys::rtcSetBufferedAmountLowThreshold(self.id, amount) })?;
        Ok(())
    }

    // TODO: rtcGetAvailableAmount
}

impl<D> Drop for RtcDataChannel<D> {
    fn drop(&mut self) {
        match check(unsafe { sys::rtcDeleteDataChannel(self.id) }) {
            Err(err) => log::error!(
                "Error while dropping RtcDataChannel id={} {:p}: {}",
                self.id,
                self,
                err
            ),
            _ => (),
        }
    }
}
