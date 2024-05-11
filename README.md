# TSL2591 Rust embedded-hal driver
A platform-agnostic driver written in Rust using embedded-hal for the TSL2591 I2C ambient light sensor.

# Status
* Contains basic functionality for reading sensor data and converting to lux.
* Supports changing ADC gain modes and integration time.
* Supports interrupts with user-configurable persist filter and ADC thresholds.
* Will work on improving interface and making code Rustier

# How to Use
* See `examples/stm32-nucleo-f411re` for how to use this driver via polling.
* See `examples/stm32-nucleo-f411re-async` for how to use this driver asynchronously via Embassy.

Further documentation is currently being worked on.

# License
This driver is licensed under the MIT license and is completely free to use and modify.
