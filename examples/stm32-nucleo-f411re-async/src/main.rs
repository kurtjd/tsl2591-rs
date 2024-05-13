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
use embassy_stm32::mode::Async;
use embassy_stm32::time::Hertz;
use embassy_stm32::usart::{Config, Uart};
use embassy_stm32::{bind_interrupts, i2c, peripherals, usart};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::mutex::Mutex;

use tsl2591_rs::*;

// Mutex for UART peripheral since it is shared by tasks
type UartType = Mutex<ThreadModeRawMutex, Option<Uart<'static, peripherals::USART2, Async>>>;
static UART_MTX: UartType = Mutex::new(None);

// Likewise mutex for sensor since it is also shared
type SensorType =
    Mutex<ThreadModeRawMutex, Option<Tsl2591Async<I2c<'static, peripherals::I2C1, Async>>>>;
static TSL2591_MTX: SensorType = Mutex::new(None);

// For UART and I2C DMA handling
bind_interrupts!(struct Irqs {
    USART2 => usart::InterruptHandler<peripherals::USART2>;
    I2C1_EV => i2c::EventInterruptHandler<peripherals::I2C1>;
    I2C1_ER => i2c::ErrorInterruptHandler<peripherals::I2C1>;
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
            tsl2591
                .power_off()
                .await
                .expect("Failed to power off device");
            uart_write("Sensor powered off\r\n").await;
        } else {
            tsl2591.power_on().await.expect("Failed to power on device");
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
                .await
                .expect("Unable to clear interrupt");

            let lux = tsl2591.get_lux(true).await.expect("Failed to retrieve lux");
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

    // Configure i2c in non-blocking (async) mode via DMA
    // Sensor uses fast mode, so freq between 100k-400k
    let i2c = I2c::new(
        p.I2C1,
        p.PB8,
        p.PB9,
        Irqs,
        p.DMA1_CH7,
        p.DMA1_CH0,
        Hertz(300_000),
        Default::default(),
    );

    // Configure sensor interrupt pin
    let sensor_int = ExtiInput::new(p.PC0, p.EXTI0, Pull::Up);

    /* Change settings to something other than default.
     * We use Tsl2591Async here, but we can also use Tsl2591 with Embassy
     * if we want blocking I2C calls. Would also need to instantiate i2c with new_blocking
     */
    let mut tsl2591 = Tsl2591Async::new(i2c).await.expect("Failed to init sensor");
    info!("Sensor initialized");

    tsl2591
        .set_again(Gain::Med)
        .await
        .expect("Failed to set sensor gain");
    tsl2591
        .set_atime(Integration::T600ms)
        .await
        .expect("Failed to set sensor integration time");
    info!("Sensor gain set to Med, integration time set to 600ms");

    // Enable interrupts from sensor
    tsl2591
        .enable_interrupt(true)
        .await
        .expect("Failed to enable interrupt");
    tsl2591
        .set_persist(Persist::F1)
        .await
        .expect("Failed to set persist filter");
    tsl2591
        .set_threshold(0, 20_000)
        .await
        .expect("Failed to set threshold");
    {
        *(TSL2591_MTX.lock().await) = Some(tsl2591);
    }
    info!("Sensor interrupt enabled, will trigger for every cycle where raw vis exceeds 20,000");

    // Finally spawn tasks for reading data and toggling power to sensor
    unwrap!(spawner.spawn(sensor_toggle_power(button)));
    unwrap!(spawner.spawn(sensor_read(sensor_int)));
}
