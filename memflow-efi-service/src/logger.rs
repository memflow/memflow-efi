use core::{
    fmt,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

use x86_64::instructions::port::PortWriteOnly;

pub struct MemReg<const N: usize> {
    reg: [u8; N],
    pos: usize,
}

impl<const N: usize> MemReg<N> {
    const fn new() -> Self {
        Self {
            reg: [0; N],
            pos: 0,
        }
    }
}

impl<const N: usize> fmt::Write for MemReg<N> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let bytes = s.as_bytes();
        if self.pos + bytes.len() + 1 >= N {
            self.pos = 0;
        }

        for b in bytes.iter() {
            self.reg[self.pos] = *b;
            self.pos = (self.pos + 1) % N;
        }

        self.reg[self.pos] = 0;

        Ok(())
    }
}

pub struct Serial;

#[no_mangle]
pub static mut MEM_LOGGER: MemReg<0x1000> = MemReg::new();

pub static mut PORT: PortWriteOnly<u8> = PortWriteOnly::new(0x3E8);

// TODO: feature flag
pub static mut MEM_LOGGING: bool = true;

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
        unsafe {
            if $crate::logger::MEM_LOGGING {
                write!(&mut $crate::logger::MEM_LOGGER, "{:5} - ", $loglevel).unwrap();
                writeln!(&mut $crate::logger::MEM_LOGGER, $($arg)*).unwrap();
            } else {
                write!($crate::logger::Serial, "{:5} - ", $loglevel).unwrap();
                writeln!($crate::logger::Serial, $($arg)*).unwrap();
            }
        }
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
