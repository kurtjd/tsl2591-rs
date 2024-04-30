#![no_std]
#![no_main]

use core::cell::{Cell, RefCell};
use cortex_m::interrupt::Mutex;
use cortex_m_rt::entry;
use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};
use stm32f4xx_hal::{
    gpio::{self, Edge, Input},
    i2c::{DutyCycle, I2c, Mode},
    pac::{self, interrupt},
    prelude::*,
};
use tsl2591_rs::{Gain, Integration, Lux, Persist, Tsl2591};

// Must be global since needed by ISR
static INT_PIN: Mutex<RefCell<Option<gpio::PC0<Input>>>> = Mutex::new(RefCell::new(None));
static INT_FLAG: Mutex<Cell<bool>> = Mutex::new(Cell::new(false));

// All we do here is set a flag for main loop to handle and clear the interrupt bit
#[interrupt]
fn EXTI0() {
    cortex_m::interrupt::free(|cs| {
        INT_FLAG.borrow(cs).set(true);
        INT_PIN
            .borrow(cs)
            .borrow_mut()
            .as_mut()
            .unwrap()
            .clear_interrupt_pending_bit();
    });
}

#[entry]
fn main() -> ! {
    rtt_init_print!();

    // Get access to peripherals
    let mut dp = pac::Peripherals::take().expect("Failed to get STM32 peripherals");

    // Set up and constrain our clocks
    let rcc = dp.RCC.constrain();
    let clocks = rcc.cfgr.freeze();

    // Setup GPIO pins for use by I2C
    let gpiob = dp.GPIOB.split();
    let scl = gpiob.pb8;
    let sda = gpiob.pb9;

    /* TSL2591 supports fast mode upto 400 KHz.
     * Don't expect the frequency given to actually be the frequency used
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

    /* Setup GPIO pin for interrupt.
     * Sensor will pull line low for interrupt, but leave it floating otherwise.
     * Hence the need for pull-up resistor.
     */
    let gpioc = dp.GPIOC.split();
    let mut int_pin = gpioc.pc0.internal_pull_up(true);
    let mut syscfg = dp.SYSCFG.constrain();

    int_pin.make_interrupt_source(&mut syscfg);
    int_pin.trigger_on_edge(&mut dp.EXTI, Edge::Falling);
    int_pin.enable_interrupt(&mut dp.EXTI);

    unsafe {
        cortex_m::peripheral::NVIC::unmask(int_pin.interrupt());
    }
    cortex_m::interrupt::free(|cs| {
        INT_PIN.borrow(cs).replace(Some(int_pin));
    });

    // Resets the sensor, verifies has correct ID, and powers on the ADC
    let mut tsl2591 = Tsl2591::new(i2c).expect("Failed to init sensor");
    rprintln!("Sensor initialized");

    // Change settings to something other than default
    tsl2591
        .set_again(Gain::Med)
        .expect("Failed to set sensor gain");
    tsl2591
        .set_atime(Integration::T600ms)
        .expect("Failed to set sensor integration time");
    rprintln!("Sensor gain set to Med, integration time set to 600ms");

    // Enable interrupts from sensor
    tsl2591
        .enable_interrupt(true)
        .expect("Failed to enable interrupt");
    tsl2591
        .set_persist(Persist::F1)
        .expect("Failed to set persist filter");
    tsl2591
        .set_threshold(0, 20_000)
        .expect("Failed to set threshold");
    rprintln!(
        "Sensor interrupt enabled, will trigger every time raw visible light count exceeds 20,000"
    );

    /* Wait for interrupt flag to be set, then we proceed to read data.
     * Is there a better way than having to constantly enter a critical section to check flag?
     * Currently unsure, will research further.
     * Didn't want to put this in body of ISR since I2C calls are blocking.
     */
    loop {
        let mut new_data = false;
        cortex_m::interrupt::free(|cs| {
            if INT_FLAG.borrow(cs).get() {
                INT_FLAG.borrow(cs).set(false);
                new_data = true;
            }
        });

        /* Do this all outside of critical section.
         * Sensor interrupt must manually be cleared.
         */
        if new_data {
            tsl2591
                .clear_interrupt()
                .expect("Failed to clear interrupt");
            let lux: Lux = tsl2591.get_lux(true).expect("Failed to get lux");
            let lux = lux.integer as f32 + lux.fractional as f32 / 1_000_000f32;
            rprintln!("Lux: {}", lux);
        }
    }
}
