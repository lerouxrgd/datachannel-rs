use std::fmt::{self, Display};

pub type Result<T> = std::result::Result<T, Error>;

pub fn check(code: i32) -> Result<i32> {
    if code < 0 {
        Err(Error::new(code))
    } else {
        Ok(code)
    }
}

#[derive(Debug)]
pub enum Error {
    InvalidArg,
    Runtime,
    Unkown,
}

impl Error {
    fn new(code: i32) -> Self {
        match code {
            -1 => Self::InvalidArg,
            -2 => Self::Runtime,
            _ => Self::Unkown,
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::InvalidArg => write!(f, "InvalidArg"),
            Self::Runtime => write!(f, "RuntimeError"),
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
