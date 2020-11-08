use std::fmt::{self, Display};

pub type Result<T> = std::result::Result<T, Error>;

pub fn check(code: i32) -> Result<i32> {
    if code < 0 {
        Err(Error::from(code))
    } else {
        Ok(code)
    }
}

#[derive(Debug)]
pub enum Error {
    InvalidArg,
    Runtime,
    NotAvailable,
    TooSmall,
    Unkown,
}

impl From<i32> for Error {
    fn from(code: i32) -> Self {
        match code {
            -1 => Self::InvalidArg,
            -2 => Self::Runtime,
            -3 => Self::NotAvailable,
            -4 => Self::TooSmall,
            _ => Self::Unkown,
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::InvalidArg => write!(f, "InvalidArg"),
            Self::Runtime => write!(f, "RuntimeError"),
            Self::NotAvailable => write!(f, "NotAvailable"),
            Self::TooSmall => write!(f, "TooSmall"),
            Self::Unkown => write!(f, "UnknownError"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::ffi::NulError> for Error {
    fn from(_source: std::ffi::NulError) -> Self {
        Self::InvalidArg
    }
}
