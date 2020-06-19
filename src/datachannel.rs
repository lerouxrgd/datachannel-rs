use std::ffi::{c_void, CStr};
use std::os::raw::c_char;
use std::slice;

use datachannel_sys as sys;

use crate::error::{check, Result};

pub trait MakeDataChannel<D>
where
    D: DataChannel + Send,
{
    fn make(&self) -> D;
}

impl<F, D> MakeDataChannel<D> for F
where
    F: Fn() -> D,
    D: DataChannel + Send,
{
    fn make(&self) -> D {
        self()
    }
}

#[allow(unused_variables)]
pub trait DataChannel {
    fn on_open(&mut self) {}
    fn on_closed(&mut self) {}
    fn on_error(&mut self, err: &str) {}
    fn on_message(&mut self, msg: &[u8]) {}
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

            Ok(rtc_dc)
        }
    }

    unsafe extern "C" fn open_cb(ptr: *mut c_void) {
        let rtc_dc = &mut *(ptr as *mut RtcDataChannel<D>);
        rtc_dc.dc.on_open()
    }

    unsafe extern "C" fn closed_cb(ptr: *mut c_void) {
        let rtc_dc = &mut *(ptr as *mut RtcDataChannel<D>);
        rtc_dc.dc.on_closed()
    }

    unsafe extern "C" fn error_cb(err: *const c_char, ptr: *mut c_void) {
        let rtc_dc = &mut *(ptr as *mut RtcDataChannel<D>);
        let err = CStr::from_ptr(err).to_string_lossy();
        rtc_dc.dc.on_error(&err)
    }

    unsafe extern "C" fn message_cb(msg: *const c_char, size: i32, ptr: *mut c_void) {
        let rtc_dc = &mut *(ptr as *mut RtcDataChannel<D>);
        let msg = if size < 0 {
            CStr::from_ptr(msg).to_bytes()
        } else {
            slice::from_raw_parts(msg as *const u8, size as usize)
        };
        rtc_dc.dc.on_message(msg)
    }

    pub fn label(&self) -> String {
        unsafe {
            let mut buf: Vec<u8> = vec![0; 256];
            match check(sys::rtcGetDataChannelLabel(
                self.id,
                buf.as_mut_ptr() as *mut c_char,
                256 as i32,
            )) {
                Ok(_) => match String::from_utf8(buf) {
                    Ok(label) => label.trim_matches(char::from(0)).to_string(),
                    Err(err) => {
                        log::error!("Couldn't get RtcDataChannel {:p} label: {}", self, err);
                        String::default()
                    }
                },
                Err(err) => {
                    log::error!("Couldn't get RtcDataChannel {:p} label: {}", self, err);
                    String::default()
                }
            }
        }
    }

    pub fn send(&mut self, msg: &[u8]) -> Result<()> {
        check(unsafe {
            sys::rtcSendMessage(self.id, msg.as_ptr() as *const c_char, msg.len() as i32)
        })
        .map(|_| ())
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
