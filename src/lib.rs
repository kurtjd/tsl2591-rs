#![no_std]
use embedded_hal::i2c::I2c;

// Used just to combine individual bits, might have to look into the bitfield crate
macro_rules! bit {
    ($n:expr) => {
        1 << $n
    };
}

/* Currently a straight port from my Zephyr implementation.
 * Will look into a more Rusty way if there is one (perhaps using the bitfield crate?)
 */
#[allow(dead_code)]
mod chip {
    /* Useful general chip constants */
    pub const I2C_ADDR: u8 = 0x29;
    pub const DEV_ID: u8 = 0x50;
    pub const MAX_ADC: u16 = 65535;
    pub const MAX_ADC_100: u16 = 36863;
    pub const LUX_DF: u16 = 408;

    /* Available registers on the chip */
    pub mod reg {
        pub const ENABLE: u8 = 0x00;
        pub const CONFIG: u8 = 0x01;
        pub const AILTL: u8 = 0x04;
        pub const AILTH: u8 = 0x05;
        pub const AIHTL: u8 = 0x06;
        pub const AIHTH: u8 = 0x07;
        pub const NPAILTL: u8 = 0x08;
        pub const NPAILTH: u8 = 0x09;
        pub const NPAIHTL: u8 = 0x0A;
        pub const NPAIHTH: u8 = 0x0B;
        pub const PERSIST: u8 = 0x0C;
        pub const PID: u8 = 0x11;
        pub const ID: u8 = 0x12;
        pub const STATUS: u8 = 0x13;
        pub const C0DATAL: u8 = 0x14;
        pub const C0DATAH: u8 = 0x15;
        pub const C1DATAL: u8 = 0x16;
        pub const C1DATAH: u8 = 0x17;
    }

    /* Command: CMD:7 | TRANSACTION:6:5 | ADDR/SF:4:0 */
    pub mod cmd {
        pub const NORMAL: u8 = bit!(7) | bit!(5);
        pub const SPECIAL: u8 = bit!(7) | bit!(6) | bit!(5);
        pub const CLEAR_INT: u8 = SPECIAL | 0x7;
    }

    /* Enable: (0x00): NPIEN:7 | SAI:6 | Reserved:5 | AIEN:4 | Reserved:3:2 | AEN:1 | PON:0 */
    pub mod enable {
        pub const POWER_MASK: u8 = bit!(1) | bit!(0);
        pub const POWER_ON: u8 = bit!(1) | bit!(0);
        pub const POWER_OFF: u8 = 0;
        pub const AEN_MASK: u8 = bit!(1);
        pub const AEN_ON: u8 = bit!(1);
        pub const AEN_OFF: u8 = 0;
        pub const AIEN_MASK: u8 = bit!(4);
        pub const AIEN_ON: u8 = bit!(4);
        pub const AIEN_OFF: u8 = 0;
    }

    /* Config/Control: (0x01): SRESET:7 | Reserved:6 | AGAIN:5:4 | Reserved:3 | ATIME:2:0 */
    pub mod config {
        pub const SRESET: u8 = bit!(7);
        pub const AGAIN_MASK: u8 = bit!(5) | bit!(4);
        pub const ATIME_MASK: u8 = bit!(2) | bit!(1) | bit!(0);
    }

    /* Status: (0x13): Reserved:7:6 | NPINTR:5 | AINT:4 | Reserved:3:1 | AVALID:0 */
    pub mod status {
        pub const AVALID_MASK: u8 = bit!(0);
    }
}

#[derive(Clone, Copy)]
pub enum Integration {
    T100ms = 0x00,
    T200ms = 0x01,
    T300ms = 0x02,
    T400ms = 0x03,
    T500ms = 0x04,
    T600ms = 0x05,
}

#[derive(Clone, Copy)]
pub enum Gain {
    Low = 0x00,
    Med = 0x10,
    High = 0x20,
    Max = 0x30,
}

#[derive(Clone, Copy)]
pub enum Persist {
    F0 = 0x00,
    F1 = 0x01,
    F2 = 0x02,
    F3 = 0x03,
    F5 = 0x04,
    F10 = 0x05,
    F15 = 0x06,
    F20 = 0x07,
    F25 = 0x08,
    F30 = 0x09,
    F35 = 0x0A,
    F40 = 0x0B,
    F45 = 0x0C,
    F50 = 0x0D,
    F55 = 0x0E,
    F60 = 0x0F,
}

#[derive(Clone, Copy, Debug)]
pub struct AlsData {
    pub visible: u16,
    pub infrared: u16,
}

// To get float value, use: integer + fractional/1_000_000
pub struct Lux {
    // Integer component of lux
    pub integer: i32,

    // Fractional component of lux (in one-millionth parts)
    pub fractional: i32,
}

#[derive(Clone, Copy, Debug)]
pub enum Error<E> {
    I2cError(E),
    InvalidId(u8),
    AdcSaturated(AlsData),
    CycleIncomplete,
}

impl<E> From<E> for Error<E> {
    fn from(error: E) -> Self {
        Error::I2cError(error)
    }
}

pub struct Tsl2591<I> {
    i2c: I,
    again: u16,
    atime: u16,
    pub powered_on: bool,
}

impl<I> Tsl2591<I>
where
    I: I2c,
{
    fn map_again(again: Gain) -> u16 {
        match again {
            Gain::Low => 1,
            Gain::Med => 25,
            Gain::High => 400,
            Gain::Max => 9200,
        }
    }

    fn map_atime(atime: Integration) -> u16 {
        match atime {
            Integration::T100ms => 100,
            Integration::T200ms => 200,
            Integration::T300ms => 300,
            Integration::T400ms => 400,
            Integration::T500ms => 500,
            Integration::T600ms => 600,
        }
    }

    pub fn new(i2c: I) -> Result<Tsl2591<I>, Error<I::Error>> {
        let mut tsl2591 = Tsl2591 {
            i2c,
            again: Self::map_again(Gain::Low),
            atime: Self::map_atime(Integration::T100ms),
            powered_on: false,
        };
        tsl2591.reset()?;

        let id = tsl2591.get_id()?;
        if id != chip::DEV_ID {
            return Err(Error::InvalidId(id));
        }
        tsl2591.power_on()?;

        Ok(tsl2591)
    }

    pub fn write(&mut self, reg: u8, val: u8) -> Result<(), Error<I::Error>> {
        self.i2c
            .write(chip::I2C_ADDR, &[chip::cmd::NORMAL | reg, val])?;
        Ok(())
    }

    pub fn read(&mut self, reg: u8, buf: &mut [u8]) -> Result<(), Error<I::Error>> {
        self.i2c
            .write_read(chip::I2C_ADDR, &[chip::cmd::NORMAL | reg], buf)?;
        Ok(())
    }

    pub fn update(&mut self, reg: u8, mask: u8, val: u8) -> Result<(), Error<I::Error>> {
        let mut old_value = [0u8; 1];
        self.read(reg, &mut old_value)?;

        let new_value = (old_value[0] & !mask) | (val & mask);
        if new_value != old_value[0] {
            self.write(reg, new_value)?;
        }

        Ok(())
    }

    pub fn power_on(&mut self) -> Result<(), Error<I::Error>> {
        self.update(
            chip::reg::ENABLE,
            chip::enable::POWER_MASK,
            chip::enable::POWER_ON,
        )?;

        self.powered_on = true;
        Ok(())
    }

    pub fn power_off(&mut self) -> Result<(), Error<I::Error>> {
        self.update(
            chip::reg::ENABLE,
            chip::enable::POWER_MASK,
            chip::enable::POWER_OFF,
        )?;

        self.powered_on = false;
        Ok(())
    }

    pub fn reset(&mut self) -> Result<(), Error<I::Error>> {
        self.power_off()?;
        self.write(chip::reg::CONFIG, chip::config::SRESET)?;
        self.power_on()?;

        Ok(())
    }

    pub fn get_id(&mut self) -> Result<u8, Error<I::Error>> {
        let mut device_id = [0u8; 1];
        self.read(chip::reg::ID, &mut device_id)?;
        Ok(device_id[0])
    }

    pub fn set_again(&mut self, gain: Gain) -> Result<(), Error<I::Error>> {
        self.power_off()?;
        self.update(chip::reg::CONFIG, chip::config::AGAIN_MASK, gain as u8)?;
        self.power_on()?;

        self.again = Self::map_again(gain);
        Ok(())
    }

    pub fn set_atime(&mut self, time: Integration) -> Result<(), Error<I::Error>> {
        self.power_off()?;
        self.update(chip::reg::CONFIG, chip::config::ATIME_MASK, time as u8)?;
        self.power_on()?;

        self.atime = Self::map_atime(time);
        Ok(())
    }

    pub fn set_persist(&mut self, persist: Persist) -> Result<(), Error<I::Error>> {
        self.power_off()?;
        self.write(chip::reg::PERSIST, persist as u8)?;
        self.power_on()?;

        Ok(())
    }

    pub fn set_threshold(&mut self, lower: u16, upper: u16) -> Result<(), Error<I::Error>> {
        // Is there a more idiomatic way to concatenate two arrays plus another value?
        let lower = u16::to_le_bytes(lower);
        let upper = u16::to_le_bytes(upper);
        let buf = [
            chip::cmd::NORMAL | chip::reg::AILTL,
            lower[0],
            lower[1],
            upper[0],
            upper[1],
        ];

        self.power_off()?;
        self.i2c.write(chip::I2C_ADDR, &buf)?;
        self.power_on()?;

        Ok(())
    }

    pub fn is_cycle_complete(&mut self) -> Result<bool, Error<I::Error>> {
        let mut status = [0u8; 1];
        self.read(chip::reg::STATUS, &mut status)?;

        // Checking if the AVALID bit is high (cycle complete) or not (cycle incomplete)
        if status[0] & chip::status::AVALID_MASK == 0 {
            Ok(false)
        } else {
            Ok(true)
        }
    }

    pub fn get_raw_als_data(&mut self, check_complete: bool) -> Result<AlsData, Error<I::Error>> {
        /* If the user wishes, check to make sure there is valid data ready to be read.
         * The sensor will set the AVALID bit when integration cycle is complete.
         * If it's set, read the data and re-assert the AEN bit to reset for next read.
         */
        if check_complete {
            if !self.is_cycle_complete()? {
                return Err(Error::CycleIncomplete);
            }

            // Re-assert AEN bit to check completion of next reading
            self.update(
                chip::reg::ENABLE,
                chip::enable::AEN_MASK,
                chip::enable::AEN_OFF,
            )?;
            self.update(
                chip::reg::ENABLE,
                chip::enable::AEN_MASK,
                chip::enable::AEN_ON,
            )?;
        }

        // Reads C0DATAL, C0DATAH, C1DATAL, and C1DATAH all in one shot
        let mut als_data = [0u8; 4];
        self.read(chip::reg::C0DATAL, &mut als_data)?;

        // Convert buffer to visible and infrared u16's
        let als_data = AlsData {
            visible: u16::from_le_bytes([als_data[0], als_data[1]]),
            infrared: u16::from_le_bytes([als_data[2], als_data[3]]),
        };

        // Saturation value is less when integration time is 100ms
        let max_count = if self.atime == 100 {
            chip::MAX_ADC_100
        } else {
            chip::MAX_ADC
        };

        // Return the data even if it's saturated just in case user wants to use it anyway
        if als_data.visible >= max_count || als_data.infrared >= max_count {
            Err(Error::AdcSaturated(als_data))
        } else {
            Ok(als_data)
        }
    }

    pub fn get_lux(&mut self, check_complete: bool) -> Result<Lux, Error<I::Error>> {
        // Will return early if saturated, since no point in calculating lux
        let als_data = self.get_raw_als_data(check_complete)?;

        // Will work on making this look a bit nicer
        let cpl: i64 = (self.atime as i64 * self.again as i64) * 1_000_000;
        let strength: i64 = if als_data.visible > 0 {
            (((als_data.visible as i64) - (als_data.infrared as i64))
                * (1_000_000
                    - (((als_data.infrared as i64) * 1_000_000) / (als_data.visible as i64))))
                * chip::LUX_DF as i64
        } else {
            0
        };

        /* Avoided using floating point math just in case architecture does not support it.
         * Instead return a struct representing integer and fractional components of lux.
         */
        Ok(Lux {
            integer: (strength / cpl) as i32,
            fractional: (((strength % cpl) * 1_000_000) / cpl) as i32,
        })
    }

    pub fn enable_interrupt(&mut self, enable: bool) -> Result<(), Error<I::Error>> {
        let aien = if enable {
            chip::enable::AIEN_ON
        } else {
            chip::enable::AIEN_OFF
        };

        self.update(chip::reg::ENABLE, chip::enable::AIEN_MASK, aien)?;
        Ok(())
    }

    pub fn clear_interrupt(&mut self) -> Result<(), Error<I::Error>> {
        self.i2c.write(chip::I2C_ADDR, &[chip::cmd::CLEAR_INT])?;
        Ok(())
    }
}
