// SPDX-License-Identifier: 0BSD

//! Global keyboard repeat settings and input-device lifecycle.

use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum};

use crate::app::AppData;
use crate::protocol::{
    river_input_device_v1::{self, RiverInputDeviceV1},
    river_input_manager_v1::RiverInputManagerV1,
};

use super::WindowManager;

#[derive(Debug)]
pub(super) struct InputDevice {
    proxy: RiverInputDeviceV1,
    configure: bool,
}

impl WindowManager {
    pub(super) fn configure_keyboards(&mut self) {
        for device in self.input_devices.values_mut() {
            if std::mem::take(&mut device.configure) {
                device.proxy.set_repeat_info(
                    self.config.keyboard.repeat_rate,
                    self.config.keyboard.repeat_delay,
                );
            }
        }
    }
}

impl Dispatch<RiverInputManagerV1, ()> for AppData {
    fn event(
        state: &mut Self,
        proxy: &RiverInputManagerV1,
        event: <RiverInputManagerV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        use crate::protocol::river_input_manager_v1::Event;
        match event {
            Event::InputDevice { id } => {
                state.wm.input_devices.insert(
                    id.id(),
                    InputDevice {
                        proxy: id,
                        configure: false,
                    },
                );
            }
            Event::Finished => {
                for (_, device) in state.wm.input_devices.drain() {
                    device.proxy.destroy();
                }
                proxy.destroy();
                state.river_input = None;
            }
        }
    }

    wayland_client::event_created_child!(AppData, RiverInputManagerV1, [
        crate::protocol::river_input_manager_v1::EVT_INPUT_DEVICE_OPCODE => (RiverInputDeviceV1, ())
    ]);
}

impl Dispatch<RiverInputDeviceV1, ()> for AppData {
    fn event(
        state: &mut Self,
        proxy: &RiverInputDeviceV1,
        event: <RiverInputDeviceV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        use river_input_device_v1::Event;
        match event {
            Event::Type {
                _type: WEnum::Value(river_input_device_v1::Type::Keyboard),
            } => {
                if let Some(device) = state.wm.input_devices.get_mut(&proxy.id()) {
                    device.configure = true;
                    // Device events are independent of window-management sequences.
                    state.request_manage_sequence();
                }
            }
            Event::Removed => {
                state.wm.input_devices.remove(&proxy.id());
                proxy.destroy();
            }
            Event::Type { .. } | Event::Name { .. } | Event::Done => {}
        }
    }
}
