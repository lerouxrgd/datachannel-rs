use std::convert::TryFrom;
use std::ffi::{c_void, CStr, CString};
use std::os::raw::c_char;
use std::ptr;
use std::slice;

use datachannel_sys as sys;

use crate::error::{check, Error, Result};

#[derive(Debug, Clone, Default)]
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

#[derive(Debug, Clone, Default)]
pub struct DataChannelInit {
    reliability: Reliability,
    protocol: CString,
    negotiated: bool,
    manual_stream: bool,
    stream: u16,
}

impl DataChannelInit {
    pub fn reliability(mut self, reliability: Reliability) -> Self {
        self.reliability = reliability;
        self
    }

    pub fn protocol(mut self, protocol: &str) -> Self {
        self.protocol = CString::new(protocol).unwrap();
        self
    }

    pub fn negotiated(mut self) -> Self {
        self.negotiated = true;
        self
    }

    pub fn manual_stream(mut self) -> Self {
        self.manual_stream = true;
        self
    }

    /// numeric ID 0-65534, ignored if `manual_stream` is false
    pub fn stream(mut self, stream: u16) -> Self {
        self.stream = stream;
        self
    }

    pub(crate) fn as_raw(&self) -> Result<sys::rtcDataChannelInit> {
        Ok(sys::rtcDataChannelInit {
            reliability: self.reliability.as_raw(),
            protocol: self.protocol.as_ptr(),
            negotiated: self.negotiated,
            manualStream: self.manual_stream,
            stream: self.stream,
        })
    }
}

#[allow(unused_variables)]
pub trait DataChannelHandler {
    fn on_open(&mut self) {}
    fn on_closed(&mut self) {}
    fn on_error(&mut self, err: &str) {}
    fn on_message(&mut self, msg: &[u8]) {}
    fn on_buffered_amount_low(&mut self) {}
    fn on_available(&mut self) {}
}

pub struct RtcDataChannel<D> {
    id: i32,
    dc_handler: D,
}

impl<D> RtcDataChannel<D>
where
    D: DataChannelHandler + Send,
{
    pub(crate) fn new(id: i32, dc_handler: D) -> Result<Box<Self>> {
        unsafe {
            let mut rtc_dc = Box::new(RtcDataChannel { id, dc_handler });
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

            check(sys::rtcSetAvailableCallback(
                id,
                Some(RtcDataChannel::<D>::available_cb),
            ))?;

            Ok(rtc_dc)
        }
    }

    unsafe extern "C" fn open_cb(_: i32, ptr: *mut c_void) {
        let rtc_dc = &mut *(ptr as *mut RtcDataChannel<D>);
        rtc_dc.dc_handler.on_open()
    }

    unsafe extern "C" fn closed_cb(_: i32, ptr: *mut c_void) {
        let rtc_dc = &mut *(ptr as *mut RtcDataChannel<D>);
        rtc_dc.dc_handler.on_closed()
    }

    unsafe extern "C" fn error_cb(_: i32, err: *const c_char, ptr: *mut c_void) {
        let rtc_dc = &mut *(ptr as *mut RtcDataChannel<D>);
        let err = CStr::from_ptr(err).to_string_lossy();
        rtc_dc.dc_handler.on_error(&err)
    }

    unsafe extern "C" fn message_cb(_: i32, msg: *const c_char, size: i32, ptr: *mut c_void) {
        let rtc_dc = &mut *(ptr as *mut RtcDataChannel<D>);
        let msg = if size < 0 {
            CStr::from_ptr(msg).to_bytes()
        } else {
            slice::from_raw_parts(msg as *const u8, size as usize)
        };
        rtc_dc.dc_handler.on_message(msg)
    }

    unsafe extern "C" fn buffered_amount_low_cb(_: i32, ptr: *mut c_void) {
        let rtc_dc = &mut *(ptr as *mut RtcDataChannel<D>);
        rtc_dc.dc_handler.on_buffered_amount_low()
    }

    unsafe extern "C" fn available_cb(_: i32, ptr: *mut c_void) {
        let rtc_dc = &mut *(ptr as *mut RtcDataChannel<D>);
        rtc_dc.dc_handler.on_available()
    }

    pub fn send(&mut self, msg: &[u8]) -> Result<()> {
        check(unsafe {
            sys::rtcSendMessage(self.id, msg.as_ptr() as *const c_char, msg.len() as i32)
        })
        .map(|_| ())
    }

    pub fn receive(&mut self) -> Result<Option<Vec<u8>>> {
        let mut size = 0 as i32;
        let buf_size = check(unsafe {
            sys::rtcReceiveMessage(self.id, ptr::null_mut() as *mut c_char, &mut size)
        })
        .expect("Couldn't get buffer size") as usize;

        let mut buf = vec![0; buf_size];
        unsafe {
            match check(sys::rtcReceiveMessage(
                self.id,
                buf.as_mut_ptr() as *mut c_char,
                &mut size,
            )) {
                Ok(_) => {
                    let msg = if size < 0 {
                        CStr::from_ptr(buf.as_ptr()).to_bytes()
                    } else {
                        slice::from_raw_parts(
                            buf[..size as usize].as_ptr() as *const u8,
                            size as usize,
                        )
                    };
                    Ok(Some(msg.to_vec()))
                }
                Err(Error::NotAvailable) => Ok(None),
                Err(err) => Err(err),
            }
        }
    }

    pub fn label(&self) -> String {
        let buf_size = check(unsafe {
            sys::rtcGetDataChannelLabel(self.id, ptr::null_mut() as *mut c_char, 0)
        })
        .expect("Couldn't get buffer size") as usize;

        let mut buf = vec![0; buf_size];
        match check(unsafe {
            sys::rtcGetDataChannelLabel(self.id, buf.as_mut_ptr() as *mut c_char, buf_size as i32)
        }) {
            Ok(_) => match crate::ffi_string(&buf) {
                Ok(label) => label,
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
                log::warn!(
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
        let buf_size = check(unsafe {
            sys::rtcGetDataChannelProtocol(self.id, ptr::null_mut() as *mut c_char, 0)
        })
        .expect("Couldn't get buffer size") as usize;

        let mut buf = vec![0; buf_size];
        match check(unsafe {
            sys::rtcGetDataChannelProtocol(
                self.id,
                buf.as_mut_ptr() as *mut c_char,
                buf_size as i32,
            )
        }) {
            Ok(1) => None,
            Ok(_) => match crate::ffi_string(&buf) {
                Ok(protocol) => Some(protocol),
                Err(err) => {
                    log::error!(
                        "Couldn't get protocol for RtcDataChannel id={} {:p}, {}",
                        self.id,
                        self,
                        err
                    );
                    None
                }
            },
            Err(err) => {
                log::warn!(
                    "Couldn't get protocol for RtcDataChannel id={} {:p}, {}",
                    self.id,
                    self,
                    err
                );
                None
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

    pub fn stream(&self) -> usize {
        check(unsafe { sys::rtcGetDataChannelStream(self.id) })
            .expect("Couldn't get RtcDataChannel stream") as usize
    }

    /// Number of bytes currently queued to be sent over the data channel.
    ///
    /// This method is the counterpart of [`available_amount`].
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

    /// Sets the lower threshold of `buffered_amount`.
    ///
    /// The default value is 0. When the number of buffered outgoing bytes, as indicated
    /// by [`buffered_amount`], falls to or below this value, a
    /// [`on_bufferd_amount_low`] event is fired. This event may be used, for example,
    /// to implement code which queues more messages to be sent whenever there's room to
    /// buffer them.
    pub fn set_buffered_amount_low_threshold(&mut self, amount: usize) -> Result<()> {
        let amount = i32::try_from(amount).map_err(|_| Error::InvalidArg)?;
        check(unsafe { sys::rtcSetBufferedAmountLowThreshold(self.id, amount) })?;
        Ok(())
    }

    /// Number of bytes currently queued to be consumed from the data channel.
    ///
    /// This method is the counterpart of [`buffered_amount`].
    pub fn available_amount(&self) -> usize {
        match check(unsafe { sys::rtcGetAvailableAmount(self.id) }) {
            Ok(amount) => amount as usize,
            Err(err) => {
                log::error!(
                    "Couldn't get available_amount for RtcDataChannel id={} {:p}, {}",
                    self.id,
                    self,
                    err
                );
                0
            }
        }
    }
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
