# TSL2591 Rust embedded-hal driver
A platform-agnostic driver written in Rust using embedded-hal for the TSL2591 I2C ambient light sensor.

# Status
* Contains basic functionality for reading sensor data and converting to lux.
* Supports changing ADC gain modes and integration time.
* Supports interrupts with user-configurable persist filter and ADC thresholds.
* Will work on improving interface and making code Rustier

# How to Use
Please see the NUCLEO-F411RE example for a general idea of how to use this driver. 
Further documentation is currently being worked on.

# Run
An example is given for the STM32 Nucleo-F411RE board. To run it, use the command:  
`cargo embed --work-dir examples/stm32-nucleo-f411re/`

# License
This driver is licensed under the MIT license and is completely free to use and modify.
