# slopwm development notes

Research recorded on **2026-10-06** for a Rust window manager running on River.
The repository implements scrolling columns with optional vertical stacks,
adapted from tinyrwm using River v0.4.8 protocols. It includes configurable
borders, keyboard repeat rate/delay, monitor growth direction, width steps,
and soft/true fullscreen.
Dynamic workspaces are independent per monitor and keep one empty workspace
below the occupied ones.
The Rust demo walkthrough records the original floating reference.

Read in this order:

1. [River architecture](river-architecture.md): scope and the reasoning behind
   separating compositor mechanisms from window-management policy.
2. [Protocol notes](protocol.md): manage/render sequencing, geometry, input,
   object lifetimes, and compatibility requirements.
3. [Rust demo walkthrough](rust-demo.md): the local tinyrwm reference, dependency
   requirements, code entry points, and gaps to address.
4. [Implementation plan](implementation-plan.md): current module responsibilities,
   remaining milestones, and meaningful validation scenarios.

## References and provenance

| Reference | Use |
| --- | --- |
| Isaac Freund, [Separating the Wayland Compositor and Window Manager](https://isaacfreund.com/blog/river-window-management/), published 2026-03-15 | Design rationale |
| [Online window-management specification](https://isaacfreund.com/docs/wayland/river-window-management-v1/) | Current reference; manager interface version 5 when checked |
| [Online XKB bindings specification](https://isaacfreund.com/docs/wayland/river-xkb-bindings-v1/) | Current reference; global interface version 3 when checked |
| [Online input-management specification](https://isaacfreund.com/docs/wayland/river-input-management-v1/) | Current reference; global interface version 2 when checked |
| [Local Rust demo README](/home/fernando/projects/river/tinyrwm/rust/README.md) and [source](/home/fernando/projects/river/tinyrwm/rust/src/main.rs) | Concrete Rust implementation |
| [Bundled protocol provenance](../protocol/README.md) | Released River v0.4.8 XML; management version 5, XKB version 3, and input management version 2 |
| [Local window-management XML](/home/fernando/projects/river/tinyrwm/rust/protocol/river-window-management-v1.xml) and [XKB XML](/home/fernando/projects/river/tinyrwm/rust/protocol/river-xkb-bindings-v1.xml) | Original demo specifications; interface versions 4 and 2 respectively |

The reference checkout is `/home/fernando/projects/river/tinyrwm`, at commit
`2261adfa3f5854726b8694bbacea29651e78a81d`. Its Rust source has a local import-order
change; the walkthrough describes the working copy read on the date above.
Absolute reference links are specific to this workstation.

Protocol identifiers ending in `v1` are names, not a claim that their interfaces
still have version 1. Keep the advertised version, bundled XML version, and
version actually bound by the client distinct. See [compatibility](protocol.md#compatibility).

The architecture summary comes from the article. Detailed implementation notes
come from the local demo and the bundled released XML. Module boundaries are
slopwm's organization choices; remaining milestones are proposals, not
requirements imposed by River.
