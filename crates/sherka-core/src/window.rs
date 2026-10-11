use serde::{Deserialize, Serialize};

/// Physical pixels, including negative coordinates on multi-monitor desktops.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Keeps the entire widget inside a monitor's work area after a screen change.
pub fn recover(
    saved: Option<Bounds>,
    monitors: &[Bounds],
    default_size: (u32, u32),
) -> Option<Bounds> {
    let first = *monitors.first()?;
    let b = saved.unwrap_or(Bounds {
        x: first
            .x
            .saturating_add(first.width.saturating_sub(default_size.0 + 28) as i32),
        y: first
            .y
            .saturating_add(first.height.saturating_sub(default_size.1 + 28) as i32),
        width: default_size.0,
        height: default_size.1,
    });
    let target = monitors
        .iter()
        .max_by_key(|m| intersection_area(b, **m))
        .copied()
        .unwrap_or(first);
    let width = b.width.clamp(180, 800).min(target.width);
    let height = b.height.clamp(230, 900).min(target.height);
    let x_max = i64::from(target.x) + i64::from(target.width - width);
    let y_max = i64::from(target.y) + i64::from(target.height - height);
    Some(Bounds {
        x: i64::from(b.x).clamp(i64::from(target.x), x_max) as i32,
        y: i64::from(b.y).clamp(i64::from(target.y), y_max) as i32,
        width,
        height,
    })
}

fn intersection_area(a: Bounds, b: Bounds) -> i64 {
    let right = (i64::from(a.x) + i64::from(a.width)).min(i64::from(b.x) + i64::from(b.width));
    let bottom = (i64::from(a.y) + i64::from(a.height)).min(i64::from(b.y) + i64::from(b.height));
    (right - i64::from(a.x.max(b.x))).max(0) * (bottom - i64::from(a.y.max(b.y))).max(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recovers_removed_monitor_and_preserves_negative_coordinates() {
        let monitor = Bounds {
            x: -1920,
            y: 0,
            width: 1920,
            height: 1040,
        };
        let saved = Bounds {
            x: 4000,
            y: 2200,
            width: 320,
            height: 380,
        };
        let recovered = recover(Some(saved), &[monitor], (320, 380)).unwrap();
        assert_eq!(
            recovered,
            Bounds {
                x: -320,
                y: 660,
                width: 320,
                height: 380
            }
        );
        assert_eq!(
            recover(Some(recovered), &[monitor], (320, 380)),
            Some(recovered)
        );
    }
    #[test]
    fn clamps_oversized_or_corrupt_geometry() {
        let monitor = Bounds {
            x: 0,
            y: 0,
            width: 640,
            height: 480,
        };
        let saved = Bounds {
            x: i32::MAX,
            y: i32::MIN,
            width: u32::MAX,
            height: 0,
        };
        assert_eq!(
            recover(Some(saved), &[monitor], (320, 380)).unwrap(),
            Bounds {
                x: 0,
                y: 0,
                width: 640,
                height: 230
            }
        );
        assert_eq!(recover(None, &[], (320, 380)), None);
    }
}
