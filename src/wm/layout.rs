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
    pub(super) fn split_vertical(self, count: usize, inner_gap: i64) -> Vec<Self> {
        // Allocate leftover pixels from top to bottom without losing height.
        // More rows than pixels cannot fit; give each row positive dimensions
        // and let the monitor clip the overflow. Gaps are fixed spacing between
        // rows; tiny heights keep 1px rows and let the overflow clip.
        let gap = inner_gap.max(0);
        let count = count.max(1) as i64;
        let height = i64::from(self.height);
        let available = height - gap * (count - 1);
        let base = (available / count).max(1);
        let remainder = if available >= count {
            available % count
        } else {
            0
        };
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
                y += row_height + gap;
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

/// Outer gaps inset the tiling area on every side. The caller applies this to
/// the panel-aware work area; true fullscreen bypasses it and covers the
/// whole monitor. Oversized gaps clamp the area to empty instead of inverting
/// it, so windows hide until gaps shrink.
pub(super) fn outer_area(area: OutputGeometry, outer: i32) -> OutputGeometry {
    let outer = i64::from(outer.max(0));
    if outer == 0 {
        return area;
    }
    let width = (i64::from(area.width) - 2 * outer).max(0);
    let height = (i64::from(area.height) - 2 * outer).max(0);
    OutputGeometry {
        x: coordinate(i64::from(area.x) + outer),
        y: coordinate(i64::from(area.y) + outer),
        width: width.min(i64::from(i32::MAX)) as i32,
        height: height.min(i64::from(i32::MAX)) as i32,
    }
}

/// Panels reserve vertical space only. Horizontal widths and peeks always use
/// the physical monitor width, including when a side panel reserves a zone.
pub(super) fn work_area(
    output: OutputGeometry,
    reserved: Option<OutputGeometry>,
) -> OutputGeometry {
    let Some(area) = reserved else {
        return output;
    };
    let top = i64::from(area.y).clamp(
        i64::from(output.y),
        i64::from(output.y) + i64::from(output.height.max(0)),
    );
    let bottom = (i64::from(area.y) + i64::from(area.height.max(0)))
        .clamp(top, i64::from(output.y) + i64::from(output.height.max(0)));
    OutputGeometry {
        y: coordinate(top),
        height: (bottom - top) as i32,
        ..output
    }
}

pub(super) fn dialog_tile(
    parent: TileGeometry,
    area: OutputGeometry,
    content: (i32, i32),
    border: i32,
    maximized: bool,
) -> Option<TileGeometry> {
    if area.width <= 0 || area.height <= 0 || parent.intersection(area).is_none() {
        return None;
    }
    let inset = inset_for(area.width);
    let available_width = (i64::from(area.width) - 2 * inset).max(1);
    let (width, height) = if maximized {
        (available_width, i64::from(area.height))
    } else {
        (
            (i64::from(content.0.max(1)) + 2 * i64::from(border)).clamp(1, available_width),
            (i64::from(content.1.max(1)) + 2 * i64::from(border)).clamp(1, i64::from(area.height)),
        )
    };
    let left = i64::from(area.x) + inset;
    let top = i64::from(area.y);
    let x = (i64::from(parent.x) + (i64::from(parent.width) - width) / 2)
        .clamp(left, left + available_width - width);
    let y = (i64::from(parent.y) + (i64::from(parent.height) - height) / 2)
        .clamp(top, top + i64::from(area.height) - height);
    Some(TileGeometry {
        x: coordinate(x),
        y: coordinate(y),
        width: width as i32,
        height: height as i32,
        border: border.max(0).min(((width.min(height) - 1) / 2) as i32),
    })
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

fn strip_offset(widths_px: &[i64], focused: usize, inner_gap: i64) -> i64 {
    let gap = inner_gap.max(0);
    widths_px[..focused.min(widths_px.len())]
        .iter()
        .sum::<i64>()
        + gap * focused.min(widths_px.len()) as i64
}

/// Minimal-scroll policy within the 98% logical area.
///
/// The strip may use `output.x + inset .. output.x + width - inset`, leaving a
/// 1% peek margin on each side for scrolled-away neighbors. If the focused
/// tile already fits, the previous scroll is kept; otherwise the strip moves
/// only as far as needed to bring it fully into view. Column gaps are part of
/// the strip: offsets include `inner_gap` between columns, while tile widths
/// themselves are unchanged.
pub(super) fn adjust_scroll(
    output_width: i32,
    widths_px: &[i64],
    focused: usize,
    prev_scroll: i64,
    inner_gap: i64,
) -> i64 {
    let Some(focused_width) = widths_px.get(focused).copied() else {
        return prev_scroll;
    };
    let inset = inset_for(output_width);
    let sum_before = strip_offset(widths_px, focused, inner_gap);
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

pub(super) fn centered_scroll(
    output_width: i32,
    widths_px: &[i64],
    focused: usize,
    inner_gap: i64,
) -> Option<i64> {
    let focused_width = *widths_px.get(focused)?;
    let sum_before = strip_offset(widths_px, focused, inner_gap);
    Some((i64::from(output_width) - focused_width) / 2 - sum_before)
}

pub(super) fn right_aligned_scroll(
    output_width: i32,
    widths_px: &[i64],
    focused: usize,
    inner_gap: i64,
) -> Option<i64> {
    let focused_width = *widths_px.get(focused)?;
    let sum_before = strip_offset(widths_px, focused, inner_gap);
    Some(i64::from(output_width) - inset_for(output_width) - focused_width - sum_before)
}

pub(super) fn place_tiles(
    output: OutputGeometry,
    widths_px: &[i64],
    scroll: i64,
    border: i32,
    inner_gap: i64,
) -> Vec<TileGeometry> {
    let gap = inner_gap.max(0);
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
            x += i64::from(width) + gap;
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
    inner_gap: i64,
) -> (Vec<TileGeometry>, i64) {
    if output.width <= 0 || output.height <= 0 || focused >= widths.len() {
        return (Vec::new(), prev_scroll);
    }
    let sizes = pixel_widths(output.width, widths);
    let scroll = adjust_scroll(output.width, &sizes, focused, prev_scroll, inner_gap);
    let tiles = place_tiles(output, &sizes, scroll, border, inner_gap);
    (tiles, scroll)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_reservations_only_change_vertical_geometry() {
        let output = OutputGeometry {
            x: -1000,
            y: -200,
            width: 1000,
            height: 800,
        };
        let area = work_area(
            output,
            Some(OutputGeometry {
                x: -900,
                y: -160,
                width: 900,
                height: 720,
            }),
        );
        assert_eq!(
            area,
            OutputGeometry {
                y: -160,
                height: 720,
                ..output
            }
        );
        let (tiles, scroll) = scrolling_tiles(area, &[50, 50], 0, 2, 10, 0);
        assert_eq!(scroll, 10);
        assert_eq!(
            (tiles[0].x, tiles[0].width, tiles[0].y, tiles[0].height),
            (-990, 500, -160, 720)
        );
        assert_eq!(work_area(output, None), output);
        for reserved in [
            OutputGeometry {
                y: -999,
                height: 9999,
                ..output
            },
            OutputGeometry {
                y: 5000,
                height: 1,
                ..output
            },
            OutputGeometry {
                height: -1,
                ..output
            },
        ] {
            let area = work_area(output, Some(reserved));
            assert_eq!((area.x, area.width), (output.x, output.width));
            assert!(area.y >= output.y && area.y + area.height <= output.y + output.height);
            assert!(area.height >= 0);
        }
    }

    #[test]
    fn dialogs_center_and_clip_without_changing_parent_geometry() {
        let area = OutputGeometry {
            x: -1000,
            y: -100,
            width: 1000,
            height: 700,
        };
        let parent = TileGeometry {
            x: -990,
            y: -100,
            width: 500,
            height: 700,
            border: 2,
        };
        let dialog = dialog_tile(parent, area, (300, 200), 2, false).unwrap();
        assert_eq!(dialog.content_size(), (300, 200));
        assert_eq!((dialog.x, dialog.y), (-892, 148));
        let oversized = dialog_tile(parent, area, (i32::MAX, i32::MAX), i32::MAX, false).unwrap();
        assert_eq!(
            (oversized.x, oversized.y, oversized.width, oversized.height),
            (-990, -100, 980, 700)
        );
        assert!(oversized.content_size().0 > 0 && oversized.content_size().1 > 0);
        let invisible = TileGeometry { x: 2000, ..parent };
        assert!(dialog_tile(invisible, area, (300, 200), 2, false).is_none());
        assert!(
            dialog_tile(
                parent,
                OutputGeometry { height: 0, ..area },
                (300, 200),
                2,
                false
            )
            .is_none()
        );
        let maximized = dialog_tile(parent, area, (300, 200), 2, true).unwrap();
        assert_eq!((maximized.width, maximized.height), (980, 700));
    }

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
                scrolling_tiles(OUTPUT, &[percent], 0, 2, inset_for(OUTPUT.width), 0);
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
            scrolling_tiles(OUTPUT, &[50, 98, 25], 0, 2, inset_for(OUTPUT.width), 0);
        assert_eq!(scroll, 10);
        let (after, scroll) = scrolling_tiles(OUTPUT, &[50, 98, 25], 1, 2, scroll, 0);
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
        let (first, scroll) = scrolling_tiles(OUTPUT, &[25, 25], 0, 2, inset_for(OUTPUT.width), 0);
        assert_eq!(first[0].x, -990);
        let (second, kept) = scrolling_tiles(OUTPUT, &[25, 25], 1, 2, scroll, 0);
        assert_eq!(kept, scroll);
        assert_eq!(second[0].x, -990);
        assert_eq!(second[1].x, -740);
        // A wide focused tile that overflows the right margin shifts left just
        // enough for its right edge to reach 99% of the monitor width.
        let widths = pixel_widths(OUTPUT.width, &[25, 98]);
        let moved = adjust_scroll(OUTPUT.width, &widths, 1, scroll, 0);
        assert_eq!(
            moved,
            i64::from(OUTPUT.width) - inset_for(OUTPUT.width) - 980 - 250
        );
        let (tiles, _) = scrolling_tiles(OUTPUT, &[25, 98], 1, 2, scroll, 0);
        assert_eq!(tiles[1].x + tiles[1].width, OUTPUT.x + OUTPUT.width - 10);
        // A focused tile left of the 1% margin shifts right to the inset.
        let shifted = adjust_scroll(OUTPUT.width, &widths, 0, -1000, 0);
        assert_eq!(shifted, inset_for(OUTPUT.width));
    }

    #[test]
    fn center_and_right_alignment_use_the_logical_area() {
        let widths = pixel_widths(OUTPUT.width, &[50, 25, 25]);
        let sum_before: i64 = widths[..1].iter().sum();
        assert_eq!(
            centered_scroll(OUTPUT.width, &widths, 1, 0).unwrap(),
            (i64::from(OUTPUT.width) - widths[1]) / 2 - sum_before
        );
        assert_eq!(
            right_aligned_scroll(OUTPUT.width, &widths, 1, 0).unwrap(),
            i64::from(OUTPUT.width) - inset_for(OUTPUT.width) - widths[1] - sum_before
        );
        // A 98% tile is already full width: center and right both equal the inset.
        let full = pixel_widths(OUTPUT.width, &[98]);
        assert_eq!(centered_scroll(OUTPUT.width, &full, 0, 0).unwrap(), 10);
        assert_eq!(right_aligned_scroll(OUTPUT.width, &full, 0, 0).unwrap(), 10);
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
        let column = scrolling_tiles(OUTPUT, &[50], 0, 3, inset_for(OUTPUT.width), 0).0[0];
        let rows = column.split_vertical(3, 0);
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
        .split_vertical(3, 0);
        assert!(tiny.iter().all(|tile| tile.content_size().1 > 0));
    }

    #[test]
    fn tiny_outputs_and_large_borders_still_propose_positive_content() {
        let output = OutputGeometry {
            width: 1,
            height: 1,
            ..OUTPUT
        };
        let tile = scrolling_tiles(output, &[1], 0, i32::MAX, inset_for(output.width), 0).0[0];
        assert_eq!(tile.content_size(), (1, 1));
        assert!(
            scrolling_tiles(OutputGeometry::default(), &[50], 0, 2, 0, 0)
                .0
                .is_empty()
        );
        let output = OutputGeometry {
            x: i32::MAX - 10,
            y: i32::MIN,
            ..OUTPUT
        };
        assert!(
            scrolling_tiles(output, &[98], 0, 0, inset_for(output.width), 0).0[0]
                .intersection(output)
                .is_some()
        );
    }

    #[test]
    fn outer_gaps_inset_the_tiling_area_and_clamp_to_empty() {
        assert_eq!(outer_area(OUTPUT, 0), OUTPUT);
        let area = outer_area(OUTPUT, 8);
        assert_eq!(
            area,
            OutputGeometry {
                x: OUTPUT.x + 8,
                y: OUTPUT.y + 8,
                width: OUTPUT.width - 16,
                height: OUTPUT.height - 16,
            }
        );
        // Oversized gaps never invert the area; layout then hides tiles.
        let empty = outer_area(OUTPUT, 10_000);
        assert_eq!((empty.width, empty.height), (0, 0));
        assert!(scrolling_tiles(empty, &[50], 0, 2, 0, 0).0.is_empty());
        // Negative values are treated as zero.
        assert_eq!(outer_area(OUTPUT, -5), OUTPUT);
    }

    #[test]
    fn inner_gaps_separate_columns_without_resizing_them() {
        let gap = 10;
        let (tiles, scroll) =
            scrolling_tiles(OUTPUT, &[25, 25], 0, 2, inset_for(OUTPUT.width), gap);
        assert_eq!(scroll, inset_for(OUTPUT.width));
        assert_eq!(tiles[0].width, 250);
        assert_eq!(tiles[1].width, 250);
        assert_eq!(i64::from(tiles[1].x - (tiles[0].x + tiles[0].width)), gap);
        // Focusing the second tile keeps the strip when it already fits.
        let (_, kept) = scrolling_tiles(OUTPUT, &[25, 25], 1, 2, scroll, gap);
        assert_eq!(kept, scroll);
        // Scroll offsets include gaps: the focused tile starts after one width
        // plus one gap.
        let widths = pixel_widths(OUTPUT.width, &[25, 98]);
        let moved = adjust_scroll(OUTPUT.width, &widths, 1, scroll, gap);
        assert_eq!(
            moved,
            i64::from(OUTPUT.width) - inset_for(OUTPUT.width) - 980 - (250 + gap)
        );
        let (tiles, _) = scrolling_tiles(OUTPUT, &[25, 98], 1, 2, scroll, gap);
        assert_eq!(tiles[1].x + tiles[1].width, OUTPUT.x + OUTPUT.width - 10);
        assert_eq!(
            centered_scroll(OUTPUT.width, &widths, 1, gap).unwrap(),
            (i64::from(OUTPUT.width) - widths[1]) / 2 - (250 + gap)
        );
        assert_eq!(
            right_aligned_scroll(OUTPUT.width, &widths, 1, gap).unwrap(),
            i64::from(OUTPUT.width) - inset_for(OUTPUT.width) - widths[1] - (250 + gap)
        );
    }

    #[test]
    fn inner_gaps_split_stack_height_and_keep_rows_positive() {
        let column = scrolling_tiles(OUTPUT, &[50], 0, 2, inset_for(OUTPUT.width), 0).0[0];
        let rows = column.split_vertical(2, 10);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].height + 10 + rows[1].height, OUTPUT.height);
        assert_eq!(rows[1].y, rows[0].y + rows[0].height + 10);
        assert_eq!((rows[0].x, rows[0].width), (column.x, column.width));
        // Leftover pixels still distribute top to bottom after gaps are removed.
        let tall = TileGeometry {
            height: 803,
            ..column
        }
        .split_vertical(3, 2);
        assert_eq!(
            tall.iter().map(|tile| tile.height).collect::<Vec<_>>(),
            [267, 266, 266]
        );
        for pair in tall.windows(2) {
            assert_eq!(pair[0].y + pair[0].height + 2, pair[1].y);
        }
        // Tiny heights keep positive content and let the overflow clip.
        let tiny = TileGeometry {
            height: 5,
            ..column
        }
        .split_vertical(3, 4);
        assert!(tiny.iter().all(|tile| tile.content_size().1 > 0));
        assert_eq!(tiny[1].y - (tiny[0].y + tiny[0].height), 4);
    }
}
