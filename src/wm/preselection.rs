// SPDX-License-Identifier: 0BSD

//! One-shot spawn targets and the visible direction hint for a selected tile.

use wayland_backend::client::ObjectId;

use crate::action::SpawnDirection;
use crate::protocol::river_window_v1::RiverWindowV1;

use super::{
    WindowManager,
    layout::{TileGeometry, inset_for, outer_area, pixel_widths, place_tiles},
    output::OutputGeometry,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Preselection {
    pub(super) output: ObjectId,
    pub(super) workspace: u64,
    pub(super) window: Option<RiverWindowV1>,
    pub(super) direction: SpawnDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Preview {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) width: i32,
    pub(super) height: i32,
    pub(super) direction: SpawnDirection,
}

impl WindowManager {
    pub(super) fn preselect(&mut self, direction: SpawnDirection) {
        if self.session_locked {
            return;
        }
        let Some(output) = self.active_output.as_ref() else {
            return;
        };
        let selection = Preselection {
            output: output.clone(),
            workspace: self.outputs[output].workspaces.current().id,
            window: self.outputs[output]
                .workspaces
                .current()
                .focused
                .as_ref()
                .and_then(|proxy| self.tiled_window(proxy))
                .map(|window| window.proxy.clone()),
            direction,
        };
        self.preselection = (self.preselection.as_ref() != Some(&selection)).then_some(selection);
    }

    pub(super) fn reconcile_preselection(&mut self) {
        if self.preselection.as_ref().is_some_and(|selection| {
            self.outputs.get(&selection.output).is_none_or(|output| {
                !output
                    .workspaces
                    .entries
                    .iter()
                    .any(|workspace| workspace.id == selection.workspace)
            }) || selection.window.as_ref().is_some_and(|anchor| {
                !self.windows.iter().any(|window| {
                    &window.proxy == anchor
                        && window.output.as_ref() == Some(&selection.output)
                        && window.workspace == selection.workspace
                })
            })
        }) {
            self.preselection = None;
        }
    }

    pub(super) fn preselection_preview(&self) -> Option<Preview> {
        let selection = self.preselection.as_ref()?;
        let selected_output = self.outputs.get(&selection.output)?;
        if selected_output.workspaces.current().id != selection.workspace {
            return None;
        }
        let mut output = outer_area(selected_output.work_area(), self.config.gaps.outer);
        let tile = if let Some(anchor) = &selection.window {
            let window = self.windows.iter().find(|window| &window.proxy == anchor)?;
            if window.fullscreen {
                output = selected_output.geometry;
                TileGeometry {
                    x: output.x,
                    y: output.y,
                    width: output.width,
                    height: output.height,
                    border: 0,
                }
            } else {
                window.animation.tile()?
            }
        } else {
            // An empty output has no anchor yet; preview its initial tile.
            let widths = pixel_widths(output.width, &[self.config.scrolling.default_width_percent]);
            *place_tiles(
                output,
                &widths,
                inset_for(output.width),
                0,
                i64::from(self.config.gaps.inner.max(0)),
            )
            .first()?
        };
        Preview::for_tile(tile, output, selection.direction)
    }
}

impl Preview {
    fn for_tile(
        tile: TileGeometry,
        output: OutputGeometry,
        direction: SpawnDirection,
    ) -> Option<Self> {
        // Mark the insertion side, rather than promising the post-scroll size.
        let (mut x, mut y) = (i64::from(tile.x), i64::from(tile.y));
        let (mut width, mut height) = (i64::from(tile.width), i64::from(tile.height));
        match direction {
            SpawnDirection::Left => width = (width / 2).max(1),
            SpawnDirection::Right => {
                x += width / 2;
                width -= width / 2;
            }
            SpawnDirection::Up => height = (height / 2).max(1),
            SpawnDirection::Down => {
                y += height / 2;
                height -= height / 2;
            }
        }
        let left = x.max(i64::from(output.x));
        let top = y.max(i64::from(output.y));
        let right = (x + width).min(i64::from(output.x) + i64::from(output.width));
        let bottom = (y + height).min(i64::from(output.y) + i64::from(output.height));
        (right > left && bottom > top).then_some(Self {
            x: left.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
            y: top.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
            width: (right - left) as i32,
            height: (bottom - top) as i32,
            direction,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opposite_previews_cover_odd_sized_tiles_without_gaps() {
        let output = OutputGeometry {
            x: -500,
            y: -40,
            width: 1001,
            height: 801,
        };
        let tile = TileGeometry {
            x: output.x,
            y: output.y,
            width: output.width,
            height: output.height,
            border: 4,
        };
        let left = Preview::for_tile(tile, output, SpawnDirection::Left).unwrap();
        let right = Preview::for_tile(tile, output, SpawnDirection::Right).unwrap();
        assert_eq!(left.x, output.x);
        assert_eq!(left.x + left.width, right.x);
        assert_eq!(right.x + right.width, output.x + output.width);
        let up = Preview::for_tile(tile, output, SpawnDirection::Up).unwrap();
        let down = Preview::for_tile(tile, output, SpawnDirection::Down).unwrap();
        assert_eq!(up.y + up.height, down.y);
        assert_eq!(down.y + down.height, output.y + output.height);
    }

    #[test]
    fn previews_clip_to_their_monitor_and_keep_tiny_tiles_positive() {
        let output = OutputGeometry {
            x: -100,
            y: 0,
            width: 100,
            height: 80,
        };
        let tile = TileGeometry {
            x: -150,
            y: 0,
            width: 100,
            height: 80,
            border: 0,
        };
        assert!(Preview::for_tile(tile, output, SpawnDirection::Left).is_none());
        let right = Preview::for_tile(tile, output, SpawnDirection::Right).unwrap();
        assert_eq!((right.x, right.width), (-100, 50));
        let tiny = TileGeometry {
            x: -100,
            y: 0,
            width: 1,
            height: 1,
            border: 0,
        };
        for direction in [
            SpawnDirection::Left,
            SpawnDirection::Right,
            SpawnDirection::Up,
            SpawnDirection::Down,
        ] {
            let preview = Preview::for_tile(tiny, output, direction).unwrap();
            assert_eq!((preview.width, preview.height), (1, 1));
        }
    }
}
