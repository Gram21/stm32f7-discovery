#![feature(lang_items)]
#![feature(const_fn)]
#![feature(trusted_len)]
#![feature(asm)]
#![feature(alloc, collections)]
#![feature(try_from)]
#![feature(drop_types_in_const)]
#![feature(option_entry)]

#![no_std]

// memcpy, memmove, etc.
extern crate rlibc;
// hardware register structs with accessor methods
pub extern crate embedded_stm32f7 as board;
pub extern crate embedded;
// low level access to the cortex-m cpu
pub extern crate cortex_m;
// volatile wrapper types
extern crate volatile;
// allocator
extern crate alloc_cortex_m;
extern crate alloc;
#[macro_use]
extern crate collections;
extern crate arrayvec;
extern crate bit_field;
extern crate spin;
extern crate byteorder;
extern crate net;
extern crate font_render;

use core::fmt;

#[macro_export]
macro_rules! println {
    ($fmt:expr) => (print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => (print!(concat!($fmt, "\n"), $($arg)*));
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ({
        $crate::print(format_args!($($arg)*));
    });
}

use spin::Mutex;
use lcd::TextWriter;

static STDOUT: Mutex<Option<TextWriter<lcd::FramebufferAl88>>> = Mutex::new(None);

pub fn print(args: fmt::Arguments) {
    use core::fmt::Write;

    match *STDOUT.lock() {
        None => {},
        Some(ref mut stdout) => {
            let _ = stdout.write_fmt(args);
        },
    };
}

pub fn init_stdout(layer: lcd::Layer<lcd::FramebufferAl88>) {
    static mut LAYER: Option<lcd::Layer<lcd::FramebufferAl88>> = None;

    let mut layer = unsafe {LAYER.get_or_insert_with(|| layer)};

    let mut stdout = STDOUT.lock();
    *stdout = Some(layer.text_writer().unwrap());
}

#[macro_use]
pub mod semi_hosting;
pub mod exceptions;
pub mod interrupts;
pub mod system_clock;
pub mod sdram;
pub mod lcd;
pub mod i2c;
pub mod audio;
pub mod touch;
pub mod ethernet;
pub mod heap;

#[cfg(not(test))]
#[lang = "panic_fmt"]
#[no_mangle]
pub extern "C" fn panic_fmt(fmt: core::fmt::Arguments, file: &'static str, line: u32) -> ! {
    println_err!("\nPANIC in {} at line {}:", file, line);
    println_err!("    {}", fmt);
    loop {}
}
