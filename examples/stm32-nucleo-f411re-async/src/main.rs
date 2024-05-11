#![no_std]
#![no_main]

use core::fmt::Write;
use defmt::*;
use heapless::String;
use {defmt_rtt as _, panic_probe as _};

use embassy_executor::Spawner;
use embassy_stm32::exti::ExtiInput;
use embassy_stm32::gpio::Pull;
use embassy_stm32::i2c::I2c;
use embassy_stm32::mode::{Async, Blocking};
use embassy_stm32::time::Hertz;
use embassy_stm32::usart::{Config, Uart};
use embassy_stm32::{bind_interrupts, peripherals, usart};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::mutex::Mutex;

use tsl2591_rs::*;

// Mutex for UART peripheral since it is shared by tasks
type UartType = Mutex<ThreadModeRawMutex, Option<Uart<'static, peripherals::USART2, Async>>>;
static UART_MTX: UartType = Mutex::new(None);

// Likewise mutex for sensor since it is also shared
type SensorType =
    Mutex<ThreadModeRawMutex, Option<Tsl2591<I2c<'static, peripherals::I2C1, Blocking>>>>;
static TSL2591_MTX: SensorType = Mutex::new(None);

// For UART DMA handling
bind_interrupts!(struct Irqs {
    USART2 => usart::InterruptHandler<peripherals::USART2>;
});

// Helper function for writing to the UART peripheral
async fn uart_write(str: &str) {
    let mut usart_lk = UART_MTX.lock().await;
    let usart = unwrap!(usart_lk.as_mut());
    unwrap!(usart.write(str.as_bytes()).await);
}

/* Toggles sensor power on and off everytime user button is pressed on Nucleo board. */
#[embassy_executor::task]
async fn sensor_toggle_power(mut button: ExtiInput<'static>) {
    loop {
        button.wait_for_falling_edge().await;

        let mut tsl2591 = TSL2591_MTX.lock().await;
        let tsl2591 = unwrap!(tsl2591.as_mut());

        if tsl2591.powered_on {
            tsl2591.power_off().expect("Failed to power off device");
            uart_write("Sensor powered off\r\n").await;
        } else {
            tsl2591.power_on().expect("Failed to power on device");
            uart_write("Sensor powered on\r\n").await;
        }
    }
}

/* Waits for the sensor to generate interrupt (depending on threshold and persist filter),
 * then reads the data and converts it to a Lux value.
 */
#[embassy_executor::task]
async fn sensor_read(mut sensor_int: ExtiInput<'static>) {
    loop {
        sensor_int.wait_for_falling_edge().await;

        let mut s: String<32> = String::new();
        {
            let mut tsl2591 = TSL2591_MTX.lock().await;
            let tsl2591 = unwrap!(tsl2591.as_mut());
            tsl2591
                .clear_interrupt()
                .expect("Unable to clear interrupt");

            let lux = tsl2591.get_lux(true).unwrap();
            core::write!(
                &mut s,
                "Lux: {}\r\n",
                lux.integer as f32 + lux.fractional as f32 / 1_000_000f32
            )
            .unwrap();
        }
        uart_write(s.as_str()).await;
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());

    // Configure button to generate interrupts
    let button = ExtiInput::new(p.PC13, p.EXTI13, Pull::Up);

    // Configure UART to use DMA (non-blocking)
    let usart = unwrap!(Uart::new(
        p.USART2,
        p.PA3,
        p.PA2,
        Irqs,
        p.DMA1_CH6,
        p.DMA1_CH5,
        Config::default()
    ));
    {
        *(UART_MTX.lock().await) = Some(usart);
    }

    // Todo: Update driver to work with both blocking and non-blocking I2C
    // For now use blocking since that's what the driver uses
    let i2c = I2c::new_blocking(p.I2C1, p.PB8, p.PB9, Hertz(300_000), Default::default());

    // Configure sensor interrupt pin
    let sensor_int = ExtiInput::new(p.PC0, p.EXTI0, Pull::Up);

    // Change settings to something other than default
    let mut tsl2591 = Tsl2591::new(i2c).expect("Failed to init sensor");
    tsl2591
        .set_again(Gain::Med)
        .expect("Failed to set sensor gain");
    tsl2591
        .set_atime(Integration::T600ms)
        .expect("Failed to set sensor integration time");

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
    {
        *(TSL2591_MTX.lock().await) = Some(tsl2591);
    }

    // Finally spawn tasks for reading data and toggling power to sensor
    unwrap!(spawner.spawn(sensor_toggle_power(button)));
    unwrap!(spawner.spawn(sensor_read(sensor_int)));
}
