#![allow(dead_code)]

use svd_board::rcc::Rcc;
use svd_board::i2c1::{self, I2c1};
use gpio::{self, GpioController};

pub struct I2C(&'static mut I2c1);

#[derive(Debug)]
pub enum Error {
    Nack,
}

#[derive(Debug, Clone, Copy)]
pub struct Address(u16);

impl Address {
    pub const fn bits_7(addr: u8) -> Address {
        Address((addr as u16) << 1)
    }
}

pub fn init_pins_and_clocks(rcc: &mut Rcc, gpio: &mut GpioController) {
    // enable clocks
    rcc.apb1enr.update(|r| {
        r.set_i2c1en(true);
        r.set_i2c2en(true);
        r.set_i2c3en(true);
        r.set_i2c4en(true);
    });

    let i2c1_scl = gpio.pins.b.8.take().unwrap();
    let i2c1_sda = gpio.pins.b.9.take().unwrap();
    let i2c2_scl = gpio.pins.b.10.take().unwrap();
    let i2c2_sda = gpio.pins.b.11.take().unwrap();
    let i2c3_scl = gpio.pins.h.7.take().unwrap();
    let i2c3_sda = gpio.pins.h.8.take().unwrap();
    let i2c4_scl = gpio.pins.h.11.take().unwrap();
    let i2c4_sda = gpio.pins.d.13.take().unwrap();

    let t = gpio::Type::OpenDrain;
    let s = gpio::Speed::Medium;
    let a = gpio::AlternateFunction::AF4;
    let r = gpio::Resistor::PullUp;

    gpio.to_alternate_function(i2c1_scl, t, s, a, r);
    gpio.to_alternate_function(i2c1_sda, t, s, a, r);
    gpio.to_alternate_function(i2c2_scl, t, s, a, r);
    gpio.to_alternate_function(i2c2_sda, t, s, a, r);
    gpio.to_alternate_function(i2c3_scl, t, s, a, r);
    gpio.to_alternate_function(i2c3_sda, t, s, a, r);
    gpio.to_alternate_function(i2c4_scl, t, s, a, r);
    gpio.to_alternate_function(i2c4_sda, t, s, a, r);
}

pub fn init(i2c: &'static mut I2c1) -> I2C {
    // disable I2C peripheral
    i2c.cr1.update(|r| r.set_pe(false)); // peripheral_enable

    // configure timing register TODO: check/understand values
    i2c.timingr.update(|r| {
        r.set_presc(0x4); // timing_prescaler
        r.set_scldel(0x9); // data_setup_time
        r.set_sdadel(0x1); // data_hold_time
        r.set_sclh(0x27); // scl_high_period
        r.set_scll(0x32); // scl_low_period
    });

    // configure oar1
    i2c.oar1.update(|r| r.set_oa1en(false)); // own_address_1_enable
    i2c.oar1.update(|r| {
        r.set_oa1(0x00); // own_address_1
        r.set_oa1mode(false); // 10 bit mode
        r.set_oa1en(false); // TODO
    });

    // configure cr2
    i2c.cr2.update(|r| {
        r.set_add10(false); // 10_bit_addressing mode
        r.set_autoend(true); // automatic_end_mode
    });

    // configure oar2
    i2c.oar2.update(|r| {
        r.set_oa2en(false); // own_address_2_enable
    });

    // configure cr1
    i2c.cr1.update(|r| {
        r.set_gcen(false); // general_call
        r.set_nostretch(false); // clock_stretching_disable
        r.set_pe(true); // peripheral_enable
    });

    I2C(i2c)
}

fn icr_clear_all() -> i2c1::Icr {
    let mut clear_all = i2c1::Icr::reset_value();
    clear_all.set_alertcf(true); // alert clear flag
    clear_all.set_timoutcf(true); // timeout detection clear flag
    clear_all.set_peccf(true); // PEC error clear flag
    clear_all.set_ovrcf(true); // overrun/underrun clear flag
    clear_all.set_arlocf(true); // arbitration loss clear flag
    clear_all.set_berrcf(true); // bus error clear flag
    clear_all.set_stopcf(true); // stop detection clear flag
    clear_all.set_nackcf(true); // not acknowledge clear flag
    clear_all.set_addrcf(true); // address matched clear flag
    clear_all
}

pub struct I2cConnection<'a> {
    i2c: &'a mut I2C,
    device_address: Address,
}

impl<'a> I2cConnection<'a> {
    fn start(&mut self, read: bool, bytes: u8) {
        let mut cr2 = i2c1::Cr2::reset_value();
        cr2.set_sadd(self.device_address.0); // slave_address
        cr2.set_start(true); // start_generation
        cr2.set_rd_wrn(read); // read_transfer
        cr2.set_nbytes(bytes); // number_of_bytes
        cr2.set_autoend(false); // automatic_end_mode
        self.i2c.0.cr2.write(cr2);
    }

    fn read_bytes(&mut self, buffer: &mut[u8]) -> Result<(), Error> {
        assert_eq!(buffer.len() as u8 as usize, buffer.len(), "transfers > 255 bytes are not implemented yet");
        self.start(true, buffer.len() as u8);

        // read data from receive data register
        for b in buffer {
            self.i2c.wait_for_rxne()?;
            *b = self.i2c.0.rxdr.read().rxdata(); // receive_data
        }

        self.i2c.wait_for_transfer_complete()?;

        self.clear_status_flags();

        // reset cr2
        self.i2c.0.cr2.write(i2c1::Cr2::reset_value());

        Ok(())
    }

    fn pre(&mut self) {
        self.clear_status_flags();
        // flush transmit data register
        self.i2c.0.isr.update(|r| r.set_txe(true)); // flush_txdr
    }

    fn clear_status_flags(&mut self) {
        let clear_all = icr_clear_all();
        self.i2c.0.icr.write(clear_all);
    }

    pub fn read(&mut self, register_address: u16) -> Result<u16, Error> {
        self.pre();

        self.start(false, 2);

        // write high address byte to transmit data register
        try!(self.i2c.wait_for_txis());
        self.i2c.0.txdr.update(|r| r.set_txdata((register_address >> 8) as u8)); // transmit_data

        // write low address byte to transmit data register
        try!(self.i2c.wait_for_txis());
        self.i2c.0.txdr.update(|r| r.set_txdata(register_address as u8)); // transmit_data

        try!(self.i2c.wait_for_transfer_complete());

        self.clear_status_flags();

        let mut buf = [0; 2];
        self.read_bytes(&mut buf)?;

        Ok((buf[0] as u16) << 8 | buf[1] as u16)
    }

    pub fn write(&mut self, register_address: u16, value: u16) -> Result<(), Error> {
        self.pre();

        self.start(false, 4);

        // write high address byte to transmit data register
        try!(self.i2c.wait_for_txis());
        self.i2c.0.txdr.update(|r| r.set_txdata((register_address >> 8) as u8)); // transmit_data

        // write low address byte to transmit data register
        try!(self.i2c.wait_for_txis());
        self.i2c.0.txdr.update(|r| r.set_txdata(register_address as u8)); // transmit_data

        // write high value byte to transmit data register
        try!(self.i2c.wait_for_txis());
        self.i2c.0.txdr.update(|r| r.set_txdata((value >> 8) as u8)); // transmit_data

        // write low value byte to transmit data register
        try!(self.i2c.wait_for_txis());
        self.i2c.0.txdr.update(|r| r.set_txdata(value as u8)); // transmit_data

        try!(self.i2c.wait_for_transfer_complete());

        self.clear_status_flags();

        // reset cr2
        self.i2c.0.cr2.write(i2c1::Cr2::reset_value());

        Ok(())
    }
}

impl I2C {
    pub fn connect<
        F: for<'a> FnOnce(I2cConnection<'a>) -> Result<(), Error>
    >(
        &mut self,
        device_address: Address,
        f: F,
    ) -> Result<(), Error> {
        {
            let conn = I2cConnection {
                i2c: self,
                device_address: device_address,
            };
            f(conn)?;
        }
        self.stop()
    }

    pub fn stop(&mut self) -> Result<(), Error> {
        self.0.cr2.update(|r| r.set_stop(true));

        self.wait_for_stop()
    }

    pub fn update<F>(&mut self,
                     device_address: Address,
                     register_address: u16,
                     f: F)
                     -> Result<(), Error>
        where F: FnOnce(&mut u16)
    {
        self.connect(device_address, |mut conn| {
            let mut value = conn.read(register_address)?;
            f(&mut value);
            conn.write(register_address, value)
        })
    }

    /// Wait for “transmit interrupt status” flag
    fn wait_for_txis(&self) -> Result<(), Error> {
        loop {
            let isr = self.0.isr.read();
            if isr.nackf() {
                // nack_received
                return Err(Error::Nack);
            }
            if isr.txis() {
                return Ok(());
            }
        }
    }

    /// Wait for "receive data register not empty" flag
    fn wait_for_rxne(&self) -> Result<(), Error> {
        loop {
            let isr = self.0.isr.read();
            if isr.nackf() {
                // nack_received
                return Err(Error::Nack);
            }
            if isr.rxne() {
                return Ok(());
            }
        }
    }

    /// Wait for “transfer complete” flag
    fn wait_for_transfer_complete(&self) -> Result<(), Error> {
        loop {
            let isr = self.0.isr.read();
            if isr.nackf() {
                // nack_received
                return Err(Error::Nack);
            }
            if isr.tc() {
                // transfer_complete
                return Ok(());
            }
        }
    }

    /// Wait for automatically generated stop flag
    fn wait_for_stop(&self) -> Result<(), Error> {
        loop {
            let isr = self.0.isr.read();
            if isr.nackf() {
                // nack_received
                return Err(Error::Nack);
            }
            if isr.stopf() {
                // stop_detected
                return Ok(());
            }
        }
    }

    // provokes a NACK
    pub fn test_1(&mut self) {
        let mut i2c = &mut self.0;

        i2c.cr2.update(|r| {
            r.set_sadd(Address::bits_7(0b1010101).0); // slave_address
            r.set_start(true); // start_generation
            r.set_nbytes(0); // number_of_bytes
            r.set_autoend(true); // automatic_end_mode
        });

        loop {
            let isr = i2c.isr.read();
            if isr.nackf() {
                // nack_received
                break;
            }
            assert!(!isr.stopf()); // stop_detected
        }

        // clear status flags
        i2c.icr.write(icr_clear_all());
    }

    // try all addresses
    #[allow(dead_code)]
    pub fn test_2(&mut self) {
        let mut i2c = &mut self.0;

        let mut addr = 0;
        loop {
            i2c.cr2.update(|r| {
                r.set_sadd(Address::bits_7(addr).0); // slave_address
                r.set_start(true); // start_generation
                r.set_nbytes(0); // number_of_bytes
                r.set_autoend(true); // automatic_end_mode
            });

            let mut isr = i2c.isr.read();
            loop {
                if isr.nackf() || isr.stopf() {
                    // nack_received or stop_detected
                    break;
                }
                isr = i2c.isr.read();
            }

            if !isr.nackf() {
                let _x = addr;
            } else {
                while i2c.isr.read().busy() {}
                // clear status flags
                i2c.icr.write(icr_clear_all());
            }

            addr += 1;
            if addr >= 0x80 {
                return;
            }
        }
    }
}

fn panic() {
    panic!();
}
