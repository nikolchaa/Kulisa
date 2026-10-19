//! Gamepads through gilrs: buttons, sticks and connections, normalized to the names the
//! scripts see (`a b x y lb rb lt rt back start guide ls rs up down left right`; axes `lx ly
//! rx ry lt rt`, y pointing down).
//!
//! gilrs has no backend on Android and iOS; there `Gilrs::new` fails and nothing is polled.

use gilrs::{Axis, Button, EventType, Gilrs};
use kulisa_lua::Game;

/// Stick deflection below this reads as zero; the rest is rescaled to 0..1.
const DEAD_ZONE: f32 = 0.15;

pub struct Pads {
    gilrs: Option<Gilrs>,
}

fn button(b: Button) -> Option<&'static str> {
    Some(match b {
        Button::South => "a",
        Button::East => "b",
        Button::West => "x",
        Button::North => "y",
        Button::LeftTrigger => "lb",
        Button::RightTrigger => "rb",
        Button::LeftTrigger2 => "lt",
        Button::RightTrigger2 => "rt",
        Button::Select => "back",
        Button::Start => "start",
        Button::Mode => "guide",
        Button::LeftThumb => "ls",
        Button::RightThumb => "rs",
        Button::DPadUp => "up",
        Button::DPadDown => "down",
        Button::DPadLeft => "left",
        Button::DPadRight => "right",
        _ => return None,
    })
}

fn dead_zone(v: f32) -> f32 {
    if v.abs() < DEAD_ZONE { 0.0 } else { v.signum() * (v.abs() - DEAD_ZONE) / (1.0 - DEAD_ZONE) }
}

impl Pads {
    pub fn new() -> Self {
        let gilrs = match Gilrs::new() {
            Ok(g) => Some(g),
            Err(e) => {
                log::info!("gamepads unavailable: {e}");
                None
            }
        };
        Self { gilrs }
    }

    /// Forwards what happened since the last call to the game.
    pub fn poll(&mut self, game: &mut Game) {
        let Some(gilrs) = &mut self.gilrs else { return };
        while let Some(event) = gilrs.next_event() {
            let pad = usize::from(event.id) as u32;
            match event.event {
                EventType::Connected => {
                    let name = gilrs.gamepad(event.id).name().to_string();
                    game.gamepad_connected(pad, Some(&name));
                }
                EventType::Disconnected => game.gamepad_connected(pad, None),
                EventType::ButtonPressed(b, _) => {
                    if let Some(name) = button(b) {
                        game.gamepad_button(pad, name, true);
                    }
                }
                EventType::ButtonReleased(b, _) => {
                    if let Some(name) = button(b) {
                        game.gamepad_button(pad, name, false);
                    }
                }
                // Analog triggers report their travel as button values.
                EventType::ButtonChanged(Button::LeftTrigger2, v, _) => game.gamepad_axis(pad, "lt", v),
                EventType::ButtonChanged(Button::RightTrigger2, v, _) => game.gamepad_axis(pad, "rt", v),
                EventType::AxisChanged(axis, v, _) => match axis {
                    Axis::LeftStickX => game.gamepad_axis(pad, "lx", dead_zone(v)),
                    Axis::LeftStickY => game.gamepad_axis(pad, "ly", -dead_zone(v)),
                    Axis::RightStickX => game.gamepad_axis(pad, "rx", dead_zone(v)),
                    Axis::RightStickY => game.gamepad_axis(pad, "ry", -dead_zone(v)),
                    // Some pads report the d-pad as an axis pair.
                    Axis::DPadX => {
                        game.gamepad_button(pad, "left", v < -0.5);
                        game.gamepad_button(pad, "right", v > 0.5);
                    }
                    Axis::DPadY => {
                        game.gamepad_button(pad, "up", v > 0.5);
                        game.gamepad_button(pad, "down", v < -0.5);
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dead_zone_rescales_and_keeps_the_sign() {
        assert_eq!(dead_zone(0.1), 0.0);
        assert_eq!(dead_zone(-0.14), 0.0);
        assert!((dead_zone(1.0) - 1.0).abs() < 1e-6);
        assert!((dead_zone(-0.575) + 0.5).abs() < 1e-6);
    }

    #[test]
    fn standard_buttons_have_script_names() {
        assert_eq!(button(Button::South), Some("a"));
        assert_eq!(button(Button::DPadLeft), Some("left"));
        assert_eq!(button(Button::C), None);
    }
}
