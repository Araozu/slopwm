# River architecture

Isaac Freund's [article](https://isaacfreund.com/blog/river-window-management/)
describes River 0.4.0 separating window management into a client process.

| Responsibility | Owner |
| --- | --- |
| Kernel input routing and display submission | River |
| Combining application buffers and maintaining the rendered scene | River |
| Window arrangement, focus, keybindings, and desktop policy | slopwm |

Keeping display-server and compositor work together avoids X11's extra
communication and disagreement between input routing and the visible scene.
Window-management policy can be separated without putting the manager in every
frame or ordinary input event.

River batches policy and rendering changes. For tiled resizes, it coordinates
application responses before presenting the arrangement. Slow applications can
exceed its timeout, so perfect frames are not unconditional.

A manager crash need not end the Wayland session. Managers can be restarted or
replaced independently. The protocol targets conventional two-dimensional
desktops; VR and elaborate effects such as wobbly windows are outside the
article's scope. See the [article](https://isaacfreund.com/blog/river-window-management/)
for the original rationale and examples.

## Project boundary

Build slopwm as a Wayland **client**, using River's management protocols. Keep
the first implementation focused on window policy and its event loop. A custom
renderer, DRM backend, and compositor framework would add a separate project
outside this boundary.

Use the [local Rust demo](rust-demo.md) to establish a working baseline, then
choose slopwm's layout and configuration behavior explicitly. Those choices are
ours; the example's floating layout does not prescribe the final desktop design.
