// SPDX-License-Identifier: 0BSD

//! Render geometry transitions, independent of Wayland objects and layout policy.

use std::time::{Duration, Instant};

use super::layout::TileGeometry;

#[derive(Debug, Default)]
pub(super) struct TileAnimation {
    displayed: Option<TileGeometry>,
    transition: Option<Transition>,
}

#[derive(Debug)]
struct Transition {
    from: TileGeometry,
    to: TileGeometry,
    started: Instant,
    duration: Duration,
}

impl TileAnimation {
    pub(super) fn tile(&self) -> Option<TileGeometry> {
        self.displayed
    }

    pub(super) fn active(&self) -> bool {
        self.transition.is_some()
    }

    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }

    pub(super) fn update(
        &mut self,
        target: Option<TileGeometry>,
        now: Instant,
        duration: Duration,
    ) -> Option<TileGeometry> {
        let Some(to) = target else {
            self.clear();
            return None;
        };
        if duration.is_zero() || self.displayed.is_none() {
            self.displayed = Some(to);
            self.transition = None;
            return self.displayed;
        }
        let changed = match &self.transition {
            Some(transition) => transition.to != to || transition.duration != duration,
            None => self.displayed != Some(to),
        };
        if changed {
            // Retarget from the last presented frame, even if the compositor
            // delayed its response. Never jump to an unpresented position.
            self.transition = (self.displayed != Some(to)).then(|| Transition {
                from: self.displayed.unwrap(),
                to,
                started: now,
                duration,
            });
        }
        if let Some(transition) = &self.transition {
            let elapsed = now.saturating_duration_since(transition.started);
            if elapsed >= transition.duration {
                self.displayed = Some(transition.to);
                self.transition = None;
            } else {
                let progress = elapsed.as_secs_f64() / transition.duration.as_secs_f64();
                let eased = 1.0 - (1.0 - progress).powi(3);
                self.displayed = Some(interpolate(transition.from, transition.to, eased));
            }
        }
        self.displayed
    }
}

fn interpolate(from: TileGeometry, to: TileGeometry, progress: f64) -> TileGeometry {
    let pixel = |from: i64, to: i64| (from as f64 + (to - from) as f64 * progress).round() as i64;
    // Interpolate edges so adjacent tiles share the same rounded boundary.
    let x = pixel(i64::from(from.x), i64::from(to.x));
    let y = pixel(i64::from(from.y), i64::from(to.y));
    let right = pixel(
        i64::from(from.x) + i64::from(from.width),
        i64::from(to.x) + i64::from(to.width),
    );
    let bottom = pixel(
        i64::from(from.y) + i64::from(from.height),
        i64::from(to.y) + i64::from(to.height),
    );
    let width = (right - x).clamp(1, i64::from(i32::MAX)) as i32;
    let height = (bottom - y).clamp(1, i64::from(i32::MAX)) as i32;
    let border = pixel(i64::from(from.border), i64::from(to.border))
        .clamp(0, i64::from((width.min(height) - 1) / 2)) as i32;
    TileGeometry {
        x: x as i32,
        y: y as i32,
        width,
        height,
        border,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(x: i32, width: i32) -> TileGeometry {
        TileGeometry {
            x,
            y: -100,
            width,
            height: 800,
            border: 2,
        }
    }

    #[test]
    fn movement_is_monotonic_finishes_exactly_and_does_not_restart() {
        let start = Instant::now();
        let duration = Duration::from_millis(200);
        let mut animation = TileAnimation::default();
        let (from, to) = (tile(-800, 500), tile(10, 500));
        assert_eq!(animation.update(Some(from), start, duration), Some(from));
        assert!(!animation.active());
        animation.update(Some(to), start, duration);
        let mut previous = from.x;
        for ms in 0..=200 {
            let displayed = animation
                .update(Some(to), start + Duration::from_millis(ms), duration)
                .unwrap();
            assert!((previous..=to.x).contains(&displayed.x));
            assert_eq!(displayed.width, from.width);
            previous = displayed.x;
        }
        assert_eq!(animation.tile(), Some(to));
        assert!(!animation.active());
        assert_eq!(
            animation.update(Some(to), start + duration, duration),
            Some(to)
        );
        assert!(!animation.active());
    }

    #[test]
    fn interrupted_movement_starts_at_the_last_displayed_frame() {
        let start = Instant::now();
        let duration = Duration::from_millis(200);
        let mut animation = TileAnimation::default();
        animation.update(Some(tile(10, 500)), start, duration);
        animation.update(Some(tile(510, 500)), start, duration);
        let shown = animation.update(Some(tile(510, 500)), start + duration / 4, duration);
        let later = start + duration * 2;
        let target = tile(-400, 500);
        assert_eq!(animation.update(Some(target), later, duration), shown);
        assert!(animation.active());
        assert_eq!(
            animation.update(Some(target), later + duration, duration),
            Some(target)
        );
        assert!(!animation.active());
    }

    #[test]
    fn disabling_hiding_and_duration_reload_do_not_leave_stale_frames() {
        let start = Instant::now();
        let duration = Duration::from_millis(200);
        let mut animation = TileAnimation::default();
        let target = tile(510, 500);
        animation.update(Some(tile(10, 500)), start, duration);
        animation.update(Some(target), start, duration);
        let shown = animation.update(Some(target), start + duration / 4, duration);
        assert_eq!(
            animation.update(Some(target), start + duration / 4, duration / 2),
            shown
        );
        assert_eq!(
            animation.update(Some(target), start + duration, duration / 2),
            Some(target)
        );
        animation.update(Some(tile(10, 500)), start + duration, duration);
        assert_eq!(
            animation.update(Some(target), start + duration, Duration::ZERO),
            Some(target)
        );
        assert!(!animation.active());
        assert_eq!(animation.update(None, start + duration, duration), None);
        assert_eq!(
            animation.update(Some(target), start + duration, duration),
            Some(target)
        );
        assert!(!animation.active());
    }

    #[test]
    fn resizing_preserves_shared_edges_and_positive_content() {
        let from = tile(-513, 500);
        let to = tile(-751, 739);
        let adjacent_from = tile(from.x + from.width, 300);
        let adjacent_to = tile(to.x + to.width, 300);
        for step in 0..=200 {
            let progress = f64::from(step) / 200.0;
            let first = interpolate(from, to, progress);
            let next = interpolate(adjacent_from, adjacent_to, progress);
            assert_eq!(first.x + first.width, next.x);
            let tiny = interpolate(from, tile(i32::MIN, 1), progress);
            assert!(tiny.content_size().0 > 0);
            assert!(tiny.content_size().1 > 0);
        }
    }

    #[test]
    fn extreme_coordinates_do_not_overflow_and_clock_regressions_keep_the_origin() {
        let from = tile(i32::MIN, i32::MAX);
        let to = tile(i32::MAX, 1);
        for step in 0..=100 {
            let shown = interpolate(from, to, f64::from(step) / 100.0);
            assert!(shown.content_size().0 > 0);
        }
        let start = Instant::now();
        let mut animation = TileAnimation::default();
        let duration = Duration::from_millis(200);
        animation.update(Some(from), start, duration);
        animation.update(Some(to), start + duration, duration);
        assert_eq!(animation.update(Some(to), start, duration), Some(from));
    }
}
