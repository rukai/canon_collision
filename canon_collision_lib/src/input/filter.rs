/// use the first received stick value to reposition the current stick value around 128
pub fn stick_deadzone(current: u8, first: u8) -> u8 {
    if current > first {
        128u8.saturating_add(current - first)
    } else {
        128u8.saturating_sub(first - current)
    }
}

pub fn stick_filter(in_stick_x: u8, in_stick_y: u8) -> (f32, f32) {
    let raw_stick_x = in_stick_x as f32 - 128.0;
    let raw_stick_y = in_stick_y as f32 - 128.0;
    let angle = (raw_stick_y).atan2(raw_stick_x);

    let max_x = (angle.cos() * 80.0).trunc();
    let max_y = (angle.sin() * 80.0).trunc();
    let stick_x = if in_stick_x == 128 { // avoid raw_stick_x = 0 and thus division by zero in the atan2)
        0.0
    } else {
        abs_min(raw_stick_x, max_x) / 80.0
    };
    let stick_y = abs_min(raw_stick_y, max_y) / 80.0;

    let deadzone = 0.28;
    (
        if stick_x.abs() < deadzone { 0.0 } else { stick_x },
        if stick_y.abs() < deadzone { 0.0 } else { stick_y }
    )
}

pub fn trigger_filter(trigger: u8) -> f32 {
    let value = (trigger as f32) / 140.0;
    if value > 1.0 {
        1.0
    } else {
        value
    }
}

fn abs_min(a: f32, b: f32) -> f32 {
    if (a >= 0.0 && a > b) || (a <= 0.0 && a < b) {
        b
    } else {
        a
    }
}

#[test]
fn stick_deadzone_test() {
    // stick_deadzone(*, 0)
    assert_eq!(stick_deadzone(0, 0), 128);
    assert_eq!(stick_deadzone(1, 0), 129);
    assert_eq!(stick_deadzone(126, 0), 254);
    assert_eq!(stick_deadzone(127, 0), 255);
    assert_eq!(stick_deadzone(255, 0), 255);

    // stick_deadzone(*, 127)
    assert_eq!(stick_deadzone(0, 127), 1);
    assert_eq!(stick_deadzone(1, 127), 2);
    assert_eq!(stick_deadzone(127, 127), 128);
    assert_eq!(stick_deadzone(128, 127), 129);
    assert_eq!(stick_deadzone(129, 127), 130);
    assert_eq!(stick_deadzone(253, 127), 254);
    assert_eq!(stick_deadzone(254, 127), 255);
    assert_eq!(stick_deadzone(255, 127), 255);

    // stick_deadzone(*, 128)
    assert_eq!(stick_deadzone(0, 128), 0);
    assert_eq!(stick_deadzone(1, 128), 1);
    assert_eq!(stick_deadzone(127, 128), 127);
    assert_eq!(stick_deadzone(128, 128), 128);
    assert_eq!(stick_deadzone(129, 128), 129);
    assert_eq!(stick_deadzone(254, 128), 254);
    assert_eq!(stick_deadzone(255, 128), 255);

    // stick_deadzone(*, 129)
    assert_eq!(stick_deadzone(0, 129), 0);
    assert_eq!(stick_deadzone(1, 129), 0);
    assert_eq!(stick_deadzone(2, 129), 1);
    assert_eq!(stick_deadzone(127, 129), 126);
    assert_eq!(stick_deadzone(128, 129), 127);
    assert_eq!(stick_deadzone(129, 129), 128);
    assert_eq!(stick_deadzone(254, 129), 253);
    assert_eq!(stick_deadzone(255, 129), 254);

    // stick_deadzone(*, 255)
    assert_eq!(stick_deadzone(0, 255), 0);
    assert_eq!(stick_deadzone(127, 255), 0);
    assert_eq!(stick_deadzone(128, 255), 1);
    assert_eq!(stick_deadzone(129, 255), 2);
    assert_eq!(stick_deadzone(254, 255), 127);
    assert_eq!(stick_deadzone(255, 255), 128);
}
