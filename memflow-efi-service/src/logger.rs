use core::{
    fmt,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

use x86_64::instructions::port::PortWriteOnly;

use crate::utils::Mutex;

pub struct Serial;

pub static mut PORT: PortWriteOnly<u8> = PortWriteOnly::new(0x3f8);

static LOG_LEVEL_NAMES: [&str; 5] = ["ERROR", "WARN", "INFO", "DEBUG", "TRACE"];

#[derive(Clone, Copy)]
pub enum LogLevel {
    Error = 0,
    Warn,
    Info,
    Debug,
    Trace,
}

impl fmt::Display for LogLevel {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.pad(LOG_LEVEL_NAMES[*self as usize])
    }
}

impl fmt::Write for Serial {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            unsafe { PORT.write(b) }
        }
        Ok(())
    }
}

macro_rules! log {
    ($loglevel:expr, $($arg:tt)*) => {{
        use core::fmt::Write;
        write!(crate::logger::Serial, "{:5} - ", $loglevel).unwrap();
        writeln!(crate::logger::Serial, $($arg)*).unwrap();
    }};
}

#[macro_export]
macro_rules! debug {
    ($($args:tt)*) => (
        log!($crate::logger::LogLevel::Debug, $($args)*);
    )
}

#[macro_export]
macro_rules! warn {
    ($($args:tt)*) => (
        log!($crate::logger::LogLevel::Warn, $($args)*);
    )
}

#[macro_export]
macro_rules! info {
    ($($args:tt)*) => (
        log!($crate::logger::LogLevel::Info, $($args)*);
    )
}

#[macro_export]
macro_rules! error {
    ($($args:tt)*) => (
        log!($crate::logger::LogLevel::Error, $($args)*);
    )
}

#[macro_export]
macro_rules! trace {
    ($($args:tt)*) => (
        log!($crate::logger::LogLevel::Trace, $($args)*);
    )
}
