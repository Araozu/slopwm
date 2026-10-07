// SPDX-License-Identifier: 0BSD

//! Run the real client against an isolated, strict Wayland protocol peer.

#[test]
fn protocol_regressions() {
    let output = std::process::Command::new("python3")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/protocol.py"))
        .arg(env!("CARGO_BIN_EXE_slopwm"))
        .output()
        .expect("protocol tests require Python 3");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
