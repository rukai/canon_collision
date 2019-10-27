use std::time::Duration;
use rusb::{Context, UsbContext, DeviceHandle, Error};

use super::state::{ControllerInput, Deadzone};
use super::filter;

pub struct GCAdapter {
    pub handle: DeviceHandle<Context>,
    pub deadzones: [Deadzone; 4]
}

impl GCAdapter {
    pub fn get_adapters(context: &mut Context) -> Vec<GCAdapter> {
        let mut adapter_handles: Vec<DeviceHandle<Context>> = Vec::new();
        let devices = context.devices();
        for device in devices.unwrap().iter() {
            if let Ok(device_desc) = device.device_descriptor() {
                if device_desc.vendor_id() == 0x057E && device_desc.product_id() == 0x0337 {
                    match device.open() {
                        Ok(mut handle) => {
                            if let Ok(true) = handle.kernel_driver_active(0) {
                                handle.detach_kernel_driver(0).unwrap();
                            }
                            match handle.claim_interface(0) {
                                Ok(_) => {
                                    // Tell adapter to start reading
                                    let payload = [0x13];
                                    if let Ok(_) = handle.write_interrupt(0x2, &payload, Duration::new(1, 0)) {
                                        adapter_handles.push(handle);
                                        println!("GC adapter: Setup complete");
                                    }
                                }
                                Err(e) => println!("GC adapter: Failed to claim interface: {}", e)
                            }
                        }
                        Err(e) => {
                            GCAdapter::handle_open_error(e);
                        }
                    }
                }
            }
        }

        adapter_handles
            .into_iter()
            .map(|handle| GCAdapter { handle, deadzones: Deadzone::empty4() })
            .collect()
    }

    fn handle_open_error(e: Error) {
        let access_solution = if cfg!(target_os = "linux") { r#":
    You need to set a udev rule so that the adapter can be accessed.
    To fix this on most Linux distributions, run the following command and then restart your computer.
    echo 'SUBSYSTEM=="usb", ENV{DEVTYPE}=="usb_device", ATTRS{idVendor}=="057e", ATTRS{idProduct}=="0337", TAG+="uaccess"' | sudo tee /etc/udev/rules.d/51-gcadapter.rules"#
        } else { "" };

        let driver_solution = if cfg!(target_os = "windows") { r#":
    To use your GC adapter you must:
    1. Download and run Zadig: http://zadig.akeo.ie/
    2. Options -> List all devices
    3. In the pulldown menu, Select WUP-028
    4. On the right ensure WinUSB is selected
    5. Select Replace Driver
    6. Select yes in the dialog box
    7. Restart Canon Collision Sandbox"#
        } else { "" };

        match e {
            Error::Access => {
                println!("GC adapter: Permissions error{}", access_solution);
            }
            Error::NotSupported => {
                println!("GC adapter: Not supported error{}", driver_solution);
            }
            _ => { println!("GC adapter: Failed to open handle: {:?}", e); }
        }
    }

    /// Add 4 GC adapter controllers to inputs
    pub fn read(&mut self, inputs: &mut Vec<ControllerInput>) {
        let mut data: [u8; 37] = [0; 37];
        if let Ok(_) = self.handle.read_interrupt(0x81, &mut data, Duration::new(1, 0)) {
            for port in 0..4 {
                let plugged_in    = data[9*port+1] == 20 || data[9*port+1] == 16;
                let raw_stick_x   = data[9*port+4];
                let raw_stick_y   = data[9*port+5];
                let raw_c_stick_x = data[9*port+6];
                let raw_c_stick_y = data[9*port+7];
                let raw_l_trigger = data[9*port+8];
                let raw_r_trigger = data[9*port+9];

                if plugged_in && !self.deadzones[port].plugged_in // Only reset deadzone if controller was just plugged in
                    && raw_stick_x != 0 // first response seems to give garbage data
                {
                    self.deadzones[port] = Deadzone {
                        plugged_in: true,
                        stick_x:    raw_stick_x,
                        stick_y:    raw_stick_y,
                        c_stick_x:  raw_c_stick_x,
                        c_stick_y:  raw_c_stick_y,
                        l_trigger:  raw_l_trigger,
                        r_trigger:  raw_r_trigger,
                    };
                }
                if !plugged_in {
                    self.deadzones[port] = Deadzone::empty();
                }

                let deadzone = &self.deadzones[port];
                let (stick_x, stick_y)     = filter::stick_filter(filter::stick_deadzone(raw_stick_x,   deadzone.stick_x),  
                                                                  filter::stick_deadzone(raw_stick_y,   deadzone.stick_y));
                let (c_stick_x, c_stick_y) = filter::stick_filter(filter::stick_deadzone(raw_c_stick_x, deadzone.c_stick_x),
                                                                  filter::stick_deadzone(raw_c_stick_y, deadzone.c_stick_y));
                let l_trigger = filter::trigger_filter(raw_l_trigger.saturating_sub(deadzone.l_trigger));
                let r_trigger = filter::trigger_filter(raw_r_trigger.saturating_sub(deadzone.r_trigger));

                inputs.push(ControllerInput {
                    up:    data[9*port+2] & 0b10000000 != 0,
                    down:  data[9*port+2] & 0b01000000 != 0,
                    right: data[9*port+2] & 0b00100000 != 0,
                    left:  data[9*port+2] & 0b00010000 != 0,
                    y:     data[9*port+2] & 0b00001000 != 0,
                    x:     data[9*port+2] & 0b00000100 != 0,
                    b:     data[9*port+2] & 0b00000010 != 0,
                    a:     data[9*port+2] & 0b00000001 != 0,
                    l:     data[9*port+3] & 0b00001000 != 0,
                    r:     data[9*port+3] & 0b00000100 != 0,
                    z:     data[9*port+3] & 0b00000010 != 0,
                    start: data[9*port+3] & 0b00000001 != 0,
                    stick_x,
                    stick_y,
                    c_stick_x,
                    c_stick_y,
                    l_trigger,
                    r_trigger,
                    plugged_in,
                });
            }
        }
        else {
            inputs.push(ControllerInput::empty());
            inputs.push(ControllerInput::empty());
            inputs.push(ControllerInput::empty());
            inputs.push(ControllerInput::empty());
        }
    }
}
