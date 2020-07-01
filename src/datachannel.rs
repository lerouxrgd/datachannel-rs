use std::convert::TryFrom;
use std::ffi::{c_void, CStr};
use std::os::raw::c_char;
use std::slice;

use datachannel_sys as sys;

use crate::error::{check, Error, Result};

pub trait MakeDataChannel<D>
where
    D: DataChannel + Send,
{
    fn make(&mut self) -> D;
}

impl<F, D> MakeDataChannel<D> for F
where
    F: FnMut() -> D,
    D: DataChannel + Send,
{
    fn make(&mut self) -> D {
        self()
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

    unsafe extern "C" fn open_cb(ptr: *mut c_void) {
        if ptr.is_null() {
            log::error!("Invalid user pointer (null) in open_cb");
            return;
        }
        let rtc_dc = &mut *(ptr as *mut RtcDataChannel<D>);
        rtc_dc.dc.on_open()
    }

    unsafe extern "C" fn closed_cb(ptr: *mut c_void) {
        if ptr.is_null() {
            log::error!("Invalid user pointer (null) in closed_cb");
            return;
        }
        let rtc_dc = &mut *(ptr as *mut RtcDataChannel<D>);
        rtc_dc.dc.on_closed()
    }

    unsafe extern "C" fn error_cb(err: *const c_char, ptr: *mut c_void) {
        if ptr.is_null() {
            log::error!("Invalid user pointer (null) in error_cb");
            return;
        }
        let rtc_dc = &mut *(ptr as *mut RtcDataChannel<D>);
        let err = CStr::from_ptr(err).to_string_lossy();
        rtc_dc.dc.on_error(&err)
    }

    unsafe extern "C" fn message_cb(msg: *const c_char, size: i32, ptr: *mut c_void) {
        if ptr.is_null() {
            log::error!("Invalid user pointer (null) in message_cb");
            return;
        }
        let rtc_dc = &mut *(ptr as *mut RtcDataChannel<D>);
        let msg = if size < 0 {
            CStr::from_ptr(msg).to_bytes()
        } else {
            slice::from_raw_parts(msg as *const u8, size as usize)
        };
        rtc_dc.dc.on_message(msg)
    }

    unsafe extern "C" fn buffered_amount_low_cb(ptr: *mut c_void) {
        if ptr.is_null() {
            log::error!("Invalid user pointer (null) in buffered_amount_low_cb");
            return;
        }
        let rtc_dc = &mut *(ptr as *mut RtcDataChannel<D>);
        rtc_dc.dc.on_buffered_amount_low()
    }

    pub fn label(&self) -> String {
        const BUF_SIZE: usize = 256;
        let mut buf: Vec<u8> = vec![0; BUF_SIZE];
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

    pub fn send(&mut self, msg: &[u8]) -> Result<()> {
        check(unsafe {
            sys::rtcSendMessage(self.id, msg.as_ptr() as *const c_char, msg.len() as i32)
        })
        .map(|_| ())
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
