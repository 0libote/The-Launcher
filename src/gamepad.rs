//! Controller input: platform backends (evdev on Linux, XInput on
//! Windows) translate gamepad buttons and the left stick into [`PadAction`]s,
//! delivered to the UI thread over an mpsc channel. `Shell::poll` drains them
//! the same way the Steam and metadata workers are drained.
//!
//! The mapping is deliberately small — d-pad/stick moves, South/Start
//! confirms, East goes back — and every action reuses the keyboard handlers,
//! so controller and keyboard behavior cannot drift apart. Holds auto-repeat
//! after a short delay; a release always stops movement, and disconnects
//! reset all held state so input can never stick.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use rune::runtime::Waker;

/// Actions a controller can trigger on the UI thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadAction {
    Move { dx: i32, dy: i32 },
    Confirm,
    Back,
}

/// Stick deflection needed to count as a direction, in normalized -1..1 units.
pub const STICK_THRESHOLD: f32 = 0.5;
/// Hold delay before a held direction starts repeating.
pub const INITIAL_DELAY: Duration = Duration::from_millis(450);
/// Repeat rate for a held direction.
pub const REPEAT_INTERVAL: Duration = Duration::from_millis(220);
/// Backend sample cadence while a controller is connected.
const TICK: Duration = Duration::from_millis(8);
/// How often a missing backend retries (unplug the dongle, plug it back in).
const RECONNECT_IDLE: Duration = Duration::from_secs(2);

/// Button snapshot both backends produce each tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PadButtons {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub confirm: bool,
    pub back: bool,
}

impl PadButtons {
    /// Merge held d-pad buttons into one direction; opposites cancel.
    pub fn direction(&self) -> Option<(i32, i32)> {
        let dx = (self.right as i32) - (self.left as i32);
        let dy = (self.down as i32) - (self.up as i32);
        (dx != 0 || dy != 0).then_some((dx, dy))
    }
}

/// Snap a normalized stick deflection to a direction. `invert_y` is for APIs
/// reporting up as positive (XInput); evdev reports down as positive.
pub fn stick_direction(x: f32, y: f32, invert_y: bool) -> Option<(i32, i32)> {
    let y = if invert_y { -y } else { y };
    let dx = if x > STICK_THRESHOLD {
        1
    } else if x < -STICK_THRESHOLD {
        -1
    } else {
        0
    };
    let dy = if y > STICK_THRESHOLD {
        1
    } else if y < -STICK_THRESHOLD {
        -1
    } else {
        0
    };
    (dx != 0 || dy != 0).then_some((dx, dy))
}

/// True on the press edge only, so a held button never double-fires.
pub fn pressed_edge(was: bool, is: bool) -> bool {
    is && !was
}

/// Auto-repeat for a held direction: fires immediately on change (press or
/// direction switch), then after [`INITIAL_DELAY`], then every
/// [`REPEAT_INTERVAL`]. Releasing (None) stops at once.
#[derive(Debug, Default)]
pub struct DirectionRepeat {
    active: Option<(i32, i32)>,
    deadline: Option<Instant>,
}

impl DirectionRepeat {
    pub fn update(&mut self, dir: Option<(i32, i32)>, now: Instant) -> Option<(i32, i32)> {
        if dir != self.active {
            self.active = dir;
            self.deadline = dir.map(|_| now + INITIAL_DELAY);
            return dir;
        }
        let d = dir?;
        if now >= self.deadline? {
            self.deadline = Some(now + REPEAT_INTERVAL);
            Some(d)
        } else {
            None
        }
    }
}

/// One backend tick: edge-detect the discrete buttons and repeat the held
/// direction, emitting at most one action per kind per tick.
pub fn tick_actions(
    prev: &PadButtons,
    next: &PadButtons,
    stick: Option<(i32, i32)>,
    repeat: &mut DirectionRepeat,
    now: Instant,
) -> Vec<PadAction> {
    let mut out = Vec::new();
    if pressed_edge(prev.confirm, next.confirm) {
        out.push(PadAction::Confirm);
    }
    if pressed_edge(prev.back, next.back) {
        out.push(PadAction::Back);
    }
    let dir = next.direction().or(stick);
    if let Some((dx, dy)) = repeat.update(dir, now) {
        out.push(PadAction::Move { dx, dy });
    }
    out
}

/// Spawn the platform backend thread. Best effort: if the OS backend is
/// missing the thread exits quietly and the launcher runs without a
/// controller, retrying on its own cadence.
pub fn spawn(tx: mpsc::Sender<PadAction>, wake: Waker) {
    if std::thread::Builder::new()
        .name("gamepad".into())
        .spawn(move || run(tx, &wake))
        .is_err()
    {
        // No thread, no controller. Keyboard navigation still works.
    }
}

fn emit(tx: &mpsc::Sender<PadAction>, wake: &Waker, action: PadAction) -> bool {
    if tx.send(action).is_ok() {
        wake();
        true
    } else {
        false
    }
}

#[cfg(target_os = "linux")]
fn run(tx: mpsc::Sender<PadAction>, wake: &Waker) {
    use evdev::{AbsoluteAxisCode, Device, EventSummary, KeyCode};

    let mut repeat = DirectionRepeat::default();
    let mut prev = PadButtons::default();
    let mut stick_raw = (0i32, 0i32);
    let mut stick_range = ((i32::MIN, i32::MAX), (i32::MIN, i32::MAX));

    let mut device: Option<Device> = None;
    loop {
        if device.is_none() {
            device = pick_gamepad();
            if device.is_none() {
                std::thread::sleep(RECONNECT_IDLE);
                continue;
            }
            // Fresh (re)connect: nothing held, nothing repeating.
            prev = PadButtons::default();
            repeat = DirectionRepeat::default();
            if let Some(dev) = device.as_ref() {
                stick_range = axis_range(dev);
                stick_raw = axis_value(dev);
            }
        }
        let Some(dev) = device.as_mut() else {
            continue;
        };
        // Collect the drain first so handling below no longer borrows the
        // device; disconnects can then drop and re-pick it freely.
        let drained: Result<Vec<_>, std::io::Error> = match dev.fetch_events() {
            Ok(events) => Ok(events.collect()),
            Err(error) => Err(error),
        };
        match drained {
            Ok(events) => {
                for ev in events {
                    match ev.destructure() {
                        EventSummary::Key(_, code, value) => {
                            let pressed = value != 0;
                            match code {
                                KeyCode::BTN_DPAD_UP => prev.up = pressed,
                                KeyCode::BTN_DPAD_DOWN => prev.down = pressed,
                                KeyCode::BTN_DPAD_LEFT => prev.left = pressed,
                                KeyCode::BTN_DPAD_RIGHT => prev.right = pressed,
                                KeyCode::BTN_SOUTH | KeyCode::BTN_START => {
                                    let next = PadButtons {
                                        confirm: pressed,
                                        ..prev
                                    };
                                    for action in tick_actions(
                                        &prev,
                                        &next,
                                        None,
                                        &mut repeat,
                                        Instant::now(),
                                    ) {
                                        if !emit(&tx, wake, action) {
                                            return;
                                        }
                                    }
                                    prev = next;
                                }
                                KeyCode::BTN_EAST => {
                                    let next = PadButtons {
                                        back: pressed,
                                        ..prev
                                    };
                                    for action in tick_actions(
                                        &prev,
                                        &next,
                                        None,
                                        &mut repeat,
                                        Instant::now(),
                                    ) {
                                        if !emit(&tx, wake, action) {
                                            return;
                                        }
                                    }
                                    prev = next;
                                }
                                _ => {}
                            }
                        }
                        EventSummary::AbsoluteAxis(_, axis, value) => {
                            if axis == AbsoluteAxisCode::ABS_X {
                                stick_raw.0 = value;
                            } else if axis == AbsoluteAxisCode::ABS_Y {
                                stick_raw.1 = value;
                            }
                        }
                        _ => {}
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                // Nothing waiting; fall through to the repeat tick below.
            }
            Err(_) => {
                // Device went away: drop it and re-pick, with state reset.
                device = None;
                prev = PadButtons::default();
                repeat = DirectionRepeat::default();
                continue;
            }
        }
        // Tick the repeat clock every pass so holds keep moving; sleep only
        // when the drain found nothing to do.
        let stick = normalize_stick(stick_raw, stick_range);
        let dir = stick_direction(stick.0, stick.1, false);
        let actions = tick_actions(&prev, &prev, dir, &mut repeat, Instant::now());
        let mut emitted = false;
        for action in actions {
            emitted = true;
            if !emit(&tx, wake, action) {
                return;
            }
        }
        if !emitted {
            std::thread::sleep(TICK);
        }
    }
}

/// First connected device that looks like a gamepad: the standard
/// South button every modern kernel maps for Xbox/PlayStation/Switch pads.
#[cfg(target_os = "linux")]
fn pick_gamepad() -> Option<evdev::Device> {
    use evdev::KeyCode;

    evdev::enumerate().find_map(|(_, device)| {
        let gamepad = device
            .supported_keys()
            .is_some_and(|keys| keys.contains(KeyCode::BTN_SOUTH));
        if !gamepad {
            return None;
        }
        if device.set_nonblocking(true).is_err() {
            return None;
        }
        Some(device)
    })
}

#[cfg(target_os = "linux")]
fn axis_range(device: &evdev::Device) -> ((i32, i32), (i32, i32)) {
    use evdev::AbsoluteAxisCode;

    let mut range = ((i32::MIN, i32::MAX), (i32::MIN, i32::MAX));
    if let Ok(infos) = device.get_absinfo() {
        for (axis, info) in infos {
            if axis == AbsoluteAxisCode::ABS_X {
                range.0 = (info.minimum(), info.maximum());
            } else if axis == AbsoluteAxisCode::ABS_Y {
                range.1 = (info.minimum(), info.maximum());
            }
        }
    }
    range
}

#[cfg(target_os = "linux")]
fn axis_value(device: &evdev::Device) -> (i32, i32) {
    use evdev::AbsoluteAxisCode;

    let mut value = (0, 0);
    if let Ok(state) = device.get_abs_state() {
        let at = |axis: AbsoluteAxisCode| state[axis.0 as usize].value;
        value = (at(AbsoluteAxisCode::ABS_X), at(AbsoluteAxisCode::ABS_Y));
    }
    value
}

#[cfg(target_os = "linux")]
fn normalize_stick(raw: (i32, i32), range: ((i32, i32), (i32, i32))) -> (f32, f32) {
    let norm = |v: i32, min: i32, max: i32| {
        if max <= min {
            0.0
        } else {
            (v - min) as f32 / (max - min) as f32 * 2.0 - 1.0
        }
    };
    (
        norm(raw.0, range.0.0, range.0.1),
        norm(raw.1, range.1.0, range.1.1),
    )
}

#[cfg(windows)]
fn run(tx: mpsc::Sender<PadAction>, wake: &Waker) {
    use rusty_xinput::XInputHandle;

    let mut repeat = DirectionRepeat::default();
    let mut prev = PadButtons::default();
    let mut handle: Option<XInputHandle> = None;
    loop {
        if handle.is_none() {
            match XInputHandle::load_default() {
                Ok(loaded) => {
                    handle = Some(loaded);
                    prev = PadButtons::default();
                    repeat = DirectionRepeat::default();
                }
                Err(_) => {
                    std::thread::sleep(RECONNECT_IDLE);
                    continue;
                }
            }
        }
        let handle_ref = handle.as_ref().expect("gamepad handle loaded");
        let mut sample: Option<(PadButtons, (f32, f32))> = None;
        for id in 0..4 {
            if let Ok(state) = handle_ref.get_state(id) {
                let buttons = PadButtons {
                    up: state.arrow_up(),
                    down: state.arrow_down(),
                    left: state.arrow_left(),
                    right: state.arrow_right(),
                    confirm: state.south_button() || state.start_button(),
                    back: state.east_button(),
                };
                sample = Some((buttons, state.left_stick_normalized()));
                break;
            }
        }
        let Some((buttons, (sx, sy))) = sample else {
            // No controller answering: reset so nothing sticks, retry later.
            prev = PadButtons::default();
            repeat = DirectionRepeat::default();
            std::thread::sleep(RECONNECT_IDLE);
            continue;
        };
        let stick = stick_direction(sx, sy, true);
        for action in tick_actions(&prev, &buttons, stick, &mut repeat, Instant::now()) {
            if !emit(&tx, wake, action) {
                return;
            }
        }
        prev = buttons;
        std::thread::sleep(TICK);
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn run(_tx: mpsc::Sender<PadAction>, _wake: &Waker) {
    // No controller backend on this platform; keyboard navigation applies.
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instant_plus(base: Instant, millis: u64) -> Instant {
        base + Duration::from_millis(millis)
    }

    #[test]
    fn stick_needs_a_deliberate_deflection() {
        assert_eq!(stick_direction(0.0, 0.0, false), None);
        assert_eq!(stick_direction(0.49, 0.0, false), None);
        assert_eq!(stick_direction(0.8, 0.0, false), Some((1, 0)));
        assert_eq!(stick_direction(0.0, -0.9, false), Some((0, -1)));
        // XInput reports up as positive; screen-up is still (0, -1).
        assert_eq!(stick_direction(0.0, 0.9, true), Some((0, -1)));
        assert_eq!(stick_direction(0.0, 0.9, false), Some((0, 1)));
    }

    #[test]
    fn dpad_opposites_cancel() {
        let both = PadButtons {
            left: true,
            right: true,
            ..PadButtons::default()
        };
        assert_eq!(both.direction(), None);
        let up = PadButtons {
            up: true,
            right: true,
            ..PadButtons::default()
        };
        assert_eq!(up.direction(), Some((1, -1)));
    }

    #[test]
    fn edges_fire_once() {
        assert!(pressed_edge(false, true));
        assert!(!pressed_edge(true, true));
        assert!(!pressed_edge(false, false));
        assert!(!pressed_edge(true, false));
    }

    #[test]
    fn repeat_fires_press_then_delay_then_rate() {
        let mut repeat = DirectionRepeat::default();
        let start = Instant::now();
        assert_eq!(repeat.update(Some((1, 0)), start), Some((1, 0)));
        assert_eq!(repeat.update(Some((1, 0)), instant_plus(start, 100)), None);
        assert_eq!(
            repeat.update(Some((1, 0)), instant_plus(start, 450)),
            Some((1, 0))
        );
        assert_eq!(repeat.update(Some((1, 0)), instant_plus(start, 500)), None);
        assert_eq!(
            repeat.update(Some((1, 0)), instant_plus(start, 670)),
            Some((1, 0))
        );
        // Switching direction fires at once; releasing stops at once.
        assert_eq!(
            repeat.update(Some((0, 1)), instant_plus(start, 671)),
            Some((0, 1))
        );
        assert_eq!(repeat.update(None, instant_plus(start, 672)), None);
        assert_eq!(repeat.update(None, instant_plus(start, 5000)), None);
    }

    #[test]
    fn tick_merges_edges_and_repeat() {
        let mut repeat = DirectionRepeat::default();
        let start = Instant::now();
        let idle = PadButtons::default();
        let down = PadButtons {
            down: true,
            confirm: true,
            ..PadButtons::default()
        };
        let actions = tick_actions(&idle, &down, None, &mut repeat, start);
        assert_eq!(
            actions,
            vec![PadAction::Confirm, PadAction::Move { dx: 0, dy: 1 },]
        );
        // Holding produces nothing until the delay passes.
        let held = tick_actions(&down, &down, None, &mut repeat, instant_plus(start, 100));
        assert!(held.is_empty());
    }
}
