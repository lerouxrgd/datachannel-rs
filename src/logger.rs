#[cfg(feature = "tracing")]
macro_rules! error {
    ($($input:tt)*) => (::tracing::error!($($input)*))
}

#[cfg(feature = "log")]
macro_rules! error {
    ($($input:tt)*) => (::log::error!($($input)*))
}

#[cfg(feature = "tracing")]
#[macro_export]
macro_rules! warn {
    ($($input:tt)*) => (::tracing::warn!($($input)*))
}

#[cfg(feature = "log")]
macro_rules! warn {
    ($($input:tt)*) => (::log::warn!($($input)*))
}

#[cfg(feature = "tracing")]
macro_rules! info {
    ($($input:tt)*) => (::tracing::info!($($input)*))
}

#[cfg(feature = "log")]
macro_rules! info {
    ($($input:tt)*) => (::log::info!($($input)*))
}

#[cfg(feature = "tracing")]
macro_rules! trace {
    ($($input:tt)*) => (::tracing::trace!($($input)*))
}

#[cfg(feature = "log")]
macro_rules! trace {
    ($($input:tt)*) => (::log::trace!($($input)*))
}

#[cfg(feature = "tracing")]
macro_rules! debug {
    ($($input:tt)*) => (::tracing::debug!($($input)*))
}

#[cfg(feature = "log")]
macro_rules! debug {
    ($($input:tt)*) => (::log::debug!($($input)*))
}