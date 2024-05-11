#![no_std]
#![no_main]

use cortex_m_rt::entry;
use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};
use stm32f4xx_hal::{
    i2c::{DutyCycle, I2c, Mode},
    pac,
    prelude::*,
};
use tsl2591_rs::{Lux, Tsl2591};

#[entry]
fn main() -> ! {
    rtt_init_print!();

    // Get access to peripherals
    let dp = pac::Peripherals::take().expect("Failed to get STM32 peripherals");
    let cp = cortex_m::peripheral::Peripherals::take().expect("Failed to get Cortex M peripherals");

    // Set up and constrain our clocks
    let rcc = dp.RCC.constrain();
    let clocks = rcc.cfgr.freeze();

    // Set delay for pause between sensor readings
    let mut delay = cp.SYST.delay(&clocks);

    // Setup GPIO pins for use by I2C
    let gpiob = dp.GPIOB.split();
    let scl = gpiob.pb8;
    let sda = gpiob.pb9;

    /* TSL2591 supports fast mode upto 400 KHz.
     * Don't expect the frequency given to actually be the frequency
     * (as confirmed via logic analyzer), using 300.kHz() here seems to work best.
     */
    let i2c = I2c::new(
        dp.I2C1,
        (scl, sda),
        Mode::Fast {
            frequency: 300.kHz(),
            duty_cycle: DutyCycle::Ratio16to9,
        },
        &clocks,
    );

    // Resets the sensor, verifies has correct ID, and powers on the ADC
    let mut tsl2591 = Tsl2591::new(i2c).expect("Failed to init sensor");
    rprintln!("Sensor initialized");

    // Change settings to something other than default
    tsl2591
        .set_again(tsl2591_rs::Gain::Med)
        .expect("Failed to set sensor gain");
    tsl2591
        .set_atime(tsl2591_rs::Integration::T200ms)
        .expect("Failed to set sensor integration time");
    rprintln!("Sensor gain set to Med, integration time set to 200ms");

    /* Retrieve sensor readings every second and calculate lux.
     * Must wait at least as long as the integration time between readings to ensure validity.
     */
    loop {
        delay.delay_ms(1000);
        let lux: Lux = tsl2591.get_lux(false).expect("Failed to get lux");
        let lux = lux.integer as f32 + lux.fractional as f32 / 1_000_000f32;
        rprintln!("Lux: {}", lux);
    }
}
