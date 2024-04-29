# TSL2591 Rust embedded-hal driver
A platform-agnostic driver written in Rust using embedded-hal for the TSL2591 I2C ambient light sensor.

# Status
* Contains basic functionality for reading sensor data and converting to lux.
* Supports changing ADC gain modes and integration time.
* Currently implementing interrupt functionality
* Will work on improving interface and making code Rustier

# Run
An example is given for the STM32 Nucleo-F411RE board. To run it, use the command:  
`cargo embed --work-dir examples/stm32-nucleo-f411re/`

# License
This driver is licensed under the MIT license and is completely free to use and modify.
