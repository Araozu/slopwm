// SPDX-License-Identifier: 0BSD

//! Scrolling columns and vertical tile geometry, independent of Wayland objects.

use super::output::OutputGeometry;

#[derive(Debug, Clone, Copy)]
pub(super) struct TileWidth {
    regular: u8,
    pub(super) soft_fullscreen: bool,
}

impl TileWidth {
    pub(super) fn new(percent: u8) -> Self {
        Self {
            regular: percent.clamp(1, 98),
            soft_fullscreen: false,
        }
    }

    pub(super) fn percent(self) -> u8 {
        if self.soft_fullscreen {
            98
        } else {
            self.regular
        }
    }

    pub(super) fn change(&mut self, delta: i16) {
        self.regular = (i16::from(self.percent()) + delta).clamp(1, 98) as u8;
        self.soft_fullscreen = false;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TileGeometry {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) width: i32,
    pub(super) height: i32,
    pub(super) border: i32,
}

impl TileGeometry {
    pub(super) fn split_vertical(self, count: usize) -> Vec<Self> {
        // Allocate leftover pixels from top to bottom without losing height.
        // More rows than pixels cannot fit; give each row positive dimensions
        // and let the monitor clip the overflow.
        let count = count.max(1) as i64;
        let height = i64::from(self.height);
        let base = (height / count).max(1);
        let remainder = if height >= count { height % count } else { 0 };
        let mut y = i64::from(self.y);
        (0..count)
            .map(|row| {
                let row_height = base + i64::from(row < remainder);
                let tile = Self {
                    y: coordinate(y),
                    height: row_height as i32,
                    border: self
                        .border
                        .min(((i64::from(self.width).min(row_height) - 1) / 2) as i32),
                    ..self
                };
                y += row_height;
                tile
            })
            .collect()
    }

    pub(super) fn content_size(self) -> (i32, i32) {
        (self.width - 2 * self.border, self.height - 2 * self.border)
    }

    pub(super) fn content_position(self) -> (i32, i32) {
        (
            coordinate(i64::from(self.x) + i64::from(self.border)),
            coordinate(i64::from(self.y) + i64::from(self.border)),
        )
    }

    pub(super) fn intersection(self, output: OutputGeometry) -> Option<(i32, i32, i32, i32)> {
        let left = i64::from(self.x).max(i64::from(output.x));
        let top = i64::from(self.y).max(i64::from(output.y));
        let right = (i64::from(self.x) + i64::from(self.width))
            .min(i64::from(output.x) + i64::from(output.width));
        let bottom = (i64::from(self.y) + i64::from(self.height))
            .min(i64::from(output.y) + i64::from(output.height));
        (right > left && bottom > top).then(|| {
            let (x, y) = self.content_position();
            (
                coordinate(left - i64::from(x)),
                coordinate(top - i64::from(y)),
                (right - left) as i32,
                (bottom - top) as i32,
            )
        })
    }
}

fn coordinate(value: i64) -> i32 {
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

pub(super) fn inset_for(output_width: i32) -> i64 {
    i64::from(output_width) / 100
}

pub(super) fn pixel_widths(output_width: i32, widths: &[u8]) -> Vec<i64> {
    widths
        .iter()
        .map(|percent| (i64::from(output_width) * i64::from(*percent) / 100).max(1))
        .collect()
}

/// Minimal-scroll policy within the 98% logical area.
///
/// The strip may use `output.x + inset .. output.x + width - inset`, leaving a
/// 1% peek margin on each side for scrolled-away neighbors. If the focused
/// tile already fits, the previous scroll is kept; otherwise the strip moves
/// only as far as needed to bring it fully into view.
pub(super) fn adjust_scroll(
    output_width: i32,
    widths_px: &[i64],
    focused: usize,
    prev_scroll: i64,
) -> i64 {
    let Some(focused_width) = widths_px.get(focused).copied() else {
        return prev_scroll;
    };
    let inset = inset_for(output_width);
    let sum_before: i64 = widths_px[..focused].iter().sum();
    let left = prev_scroll + sum_before;
    let right = left + focused_width;
    let min_left = inset;
    let max_right = i64::from(output_width) - inset;
    if left < min_left {
        min_left - sum_before
    } else if right > max_right {
        max_right - focused_width - sum_before
    } else {
        prev_scroll
    }
}

pub(super) fn centered_scroll(output_width: i32, widths_px: &[i64], focused: usize) -> Option<i64> {
    let focused_width = *widths_px.get(focused)?;
    let sum_before: i64 = widths_px[..focused].iter().sum();
    Some((i64::from(output_width) - focused_width) / 2 - sum_before)
}

pub(super) fn right_aligned_scroll(
    output_width: i32,
    widths_px: &[i64],
    focused: usize,
) -> Option<i64> {
    let focused_width = *widths_px.get(focused)?;
    let sum_before: i64 = widths_px[..focused].iter().sum();
    Some(i64::from(output_width) - inset_for(output_width) - focused_width - sum_before)
}

pub(super) fn place_tiles(
    output: OutputGeometry,
    widths_px: &[i64],
    scroll: i64,
    border: i32,
) -> Vec<TileGeometry> {
    let mut x = i64::from(output.x) + scroll;
    widths_px
        .iter()
        .map(|width| {
            let width = (*width).clamp(1, i64::from(i32::MAX)) as i32;
            let tile = TileGeometry {
                x: coordinate(x),
                y: output.y,
                width,
                height: output.height,
                border: border.min((width.min(output.height) - 1) / 2).max(0),
            };
            x += i64::from(width);
            tile
        })
        .collect()
}

pub(super) fn scrolling_tiles(
    output: OutputGeometry,
    widths: &[u8],
    focused: usize,
    border: i32,
    prev_scroll: i64,
) -> (Vec<TileGeometry>, i64) {
    if output.width <= 0 || output.height <= 0 || focused >= widths.len() {
        return (Vec::new(), prev_scroll);
    }
    let sizes = pixel_widths(output.width, widths);
    let scroll = adjust_scroll(output.width, &sizes, focused, prev_scroll);
    let tiles = place_tiles(output, &sizes, scroll, border);
    (tiles, scroll)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUTPUT: OutputGeometry = OutputGeometry {
        x: -1000,
        y: -50,
        width: 1000,
        height: 800,
    };

    #[test]
    fn soft_fullscreen_and_single_small_tile_keep_the_left_inset() {
        for percent in [25, 98] {
            let (tiles, scroll) =
                scrolling_tiles(OUTPUT, &[percent], 0, 2, inset_for(OUTPUT.width));
            let tile = tiles[0];
            assert_eq!(scroll, inset_for(OUTPUT.width));
            assert_eq!(
                (tile.x, tile.y, tile.width, tile.height),
                (-990, -50, i32::from(percent) * 10, 800)
            );
            assert_eq!(tile.content_position(), (-988, -48));
            assert_eq!(tile.content_size(), (tile.width - 4, 796));
        }
    }

    #[test]
    fn focus_scrolls_without_resizing_and_keeps_neighbor_peeks() {
        let (before, scroll) =
            scrolling_tiles(OUTPUT, &[50, 98, 25], 0, 2, inset_for(OUTPUT.width));
        assert_eq!(scroll, 10);
        let (after, scroll) = scrolling_tiles(OUTPUT, &[50, 98, 25], 1, 2, scroll);
        assert_eq!(scroll, 10 - 500);
        assert_eq!(after[1].x, -990);
        assert_eq!(after[0].intersection(OUTPUT).unwrap().2, 10);
        assert_eq!(after[2].intersection(OUTPUT).unwrap().2, 10);
        for (old, new) in before.iter().zip(&after) {
            assert_eq!(old.content_size(), new.content_size());
        }
        for pair in after.windows(2) {
            assert_eq!(pair[0].x + pair[0].width, pair[1].x);
        }
    }

    #[test]
    fn fitting_focus_keeps_scroll_and_overshoot_moves_only_what_is_needed() {
        // Two narrow tiles fit in the 98% logical area: focusing either keeps
        // the strip where it is.
        let (first, scroll) = scrolling_tiles(OUTPUT, &[25, 25], 0, 2, inset_for(OUTPUT.width));
        assert_eq!(first[0].x, -990);
        let (second, kept) = scrolling_tiles(OUTPUT, &[25, 25], 1, 2, scroll);
        assert_eq!(kept, scroll);
        assert_eq!(second[0].x, -990);
        assert_eq!(second[1].x, -740);
        // A wide focused tile that overflows the right margin shifts left just
        // enough for its right edge to reach 99% of the monitor width.
        let widths = pixel_widths(OUTPUT.width, &[25, 98]);
        let moved = adjust_scroll(OUTPUT.width, &widths, 1, scroll);
        assert_eq!(
            moved,
            i64::from(OUTPUT.width) - inset_for(OUTPUT.width) - 980 - 250
        );
        let (tiles, _) = scrolling_tiles(OUTPUT, &[25, 98], 1, 2, scroll);
        assert_eq!(tiles[1].x + tiles[1].width, OUTPUT.x + OUTPUT.width - 10);
        // A focused tile left of the 1% margin shifts right to the inset.
        let shifted = adjust_scroll(OUTPUT.width, &widths, 0, -1000);
        assert_eq!(shifted, inset_for(OUTPUT.width));
    }

    #[test]
    fn center_and_right_alignment_use_the_logical_area() {
        let widths = pixel_widths(OUTPUT.width, &[50, 25, 25]);
        let sum_before: i64 = widths[..1].iter().sum();
        assert_eq!(
            centered_scroll(OUTPUT.width, &widths, 1).unwrap(),
            (i64::from(OUTPUT.width) - widths[1]) / 2 - sum_before
        );
        assert_eq!(
            right_aligned_scroll(OUTPUT.width, &widths, 1).unwrap(),
            i64::from(OUTPUT.width) - inset_for(OUTPUT.width) - widths[1] - sum_before
        );
        // A 98% tile is already full width: center and right both equal the inset.
        let full = pixel_widths(OUTPUT.width, &[98]);
        assert_eq!(centered_scroll(OUTPUT.width, &full, 0).unwrap(), 10);
        assert_eq!(right_aligned_scroll(OUTPUT.width, &full, 0).unwrap(), 10);
    }

    #[test]
    fn widths_survive_soft_fullscreen_and_resize_is_bounded() {
        let mut width = TileWidth::new(40);
        width.soft_fullscreen = true;
        assert_eq!(width.percent(), 98);
        width.soft_fullscreen = false;
        assert_eq!(width.percent(), 40);
        width.soft_fullscreen = true;
        width.change(-10);
        assert_eq!(width.percent(), 88);
        assert!(!width.soft_fullscreen);
        width.change(97);
        assert_eq!(width.percent(), 98);
        width.change(-97);
        assert_eq!(width.percent(), 1);
    }

    #[test]
    fn stacked_windows_share_width_and_fill_height_with_leftover_pixels() {
        let column = scrolling_tiles(OUTPUT, &[50], 0, 3, inset_for(OUTPUT.width)).0[0];
        let rows = column.split_vertical(3);
        assert_eq!(
            rows.iter().map(|tile| tile.height).collect::<Vec<_>>(),
            [267, 267, 266]
        );
        assert_eq!(rows[0].y, OUTPUT.y);
        assert_eq!(rows[2].y + rows[2].height, OUTPUT.y + OUTPUT.height);
        for row in &rows {
            assert_eq!((row.x, row.width), (column.x, column.width));
            assert!(row.content_size().0 > 0 && row.content_size().1 > 0);
        }
        for pair in rows.windows(2) {
            assert_eq!(pair[0].y + pair[0].height, pair[1].y);
        }
        let tiny = TileGeometry {
            height: 2,
            ..column
        }
        .split_vertical(3);
        assert!(tiny.iter().all(|tile| tile.content_size().1 > 0));
    }

    #[test]
    fn tiny_outputs_and_large_borders_still_propose_positive_content() {
        let output = OutputGeometry {
            width: 1,
            height: 1,
            ..OUTPUT
        };
        let tile = scrolling_tiles(output, &[1], 0, i32::MAX, inset_for(output.width)).0[0];
        assert_eq!(tile.content_size(), (1, 1));
        assert!(
            scrolling_tiles(OutputGeometry::default(), &[50], 0, 2, 0)
                .0
                .is_empty()
        );
        let output = OutputGeometry {
            x: i32::MAX - 10,
            y: i32::MIN,
            ..OUTPUT
        };
        assert!(
            scrolling_tiles(output, &[98], 0, 0, inset_for(output.width)).0[0]
                .intersection(output)
                .is_some()
        );
    }
}
