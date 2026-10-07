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

pub(super) fn scrolling_tiles(
    output: OutputGeometry,
    widths: &[u8],
    focused: usize,
    border: i32,
) -> Vec<TileGeometry> {
    if output.width <= 0 || output.height <= 0 || focused >= widths.len() {
        return Vec::new();
    }
    let sizes: Vec<_> = widths
        .iter()
        .map(|percent| (i64::from(output.width) * i64::from(*percent) / 100).max(1))
        .collect();
    let inset = i64::from(output.width) / 100;
    let mut x = i64::from(output.x) + inset - sizes[..focused].iter().sum::<i64>();
    sizes
        .into_iter()
        .map(|width| {
            let width = width as i32;
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
            let tile = scrolling_tiles(OUTPUT, &[percent], 0, 2)[0];
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
        let before = scrolling_tiles(OUTPUT, &[50, 98, 25], 0, 2);
        let after = scrolling_tiles(OUTPUT, &[50, 98, 25], 1, 2);
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
        let column = scrolling_tiles(OUTPUT, &[50], 0, 3)[0];
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
        let tile = scrolling_tiles(output, &[1], 0, i32::MAX)[0];
        assert_eq!(tile.content_size(), (1, 1));
        assert!(scrolling_tiles(OutputGeometry::default(), &[50], 0, 2).is_empty());
        let output = OutputGeometry {
            x: i32::MAX - 10,
            y: i32::MIN,
            ..OUTPUT
        };
        assert!(
            scrolling_tiles(output, &[98], 0, 0)[0]
                .intersection(output)
                .is_some()
        );
    }
}
