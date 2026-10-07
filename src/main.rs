// SPDX-FileCopyrightText: © 2026 Julian Andrews
// SPDX-License-Identifier: 0BSD

//! Application entry point.

mod app;
mod protocol;
mod wm;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    app::run()
}
