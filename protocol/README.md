# Bundled River protocols

These XML files are copied unchanged from the **River v0.4.8 release**, commit
`c4b5f706314555f4846e25b8d3635631387b3fdd`, checked on 2026-10-06.
The interface versions match the latest documented specifications at that date.

| XML | Maximum interface version | Documentation |
| --- | --- | --- |
| [river-window-management-v1.xml](river-window-management-v1.xml) | 5 | [Window management](https://isaacfreund.com/docs/wayland/river-window-management-v1/) |
| [river-xkb-bindings-v1.xml](river-xkb-bindings-v1.xml) | 3 | [XKB bindings](https://isaacfreund.com/docs/wayland/river-xkb-bindings-v1/) |

Upstream source: [River v0.4.8 protocol directory](https://codeberg.org/river/river/src/tag/v0.4.8/protocol).
Each file includes its original Isaac Freund copyright and MIT license notice.

`wayland-scanner` generates Rust interfaces and client bindings during compilation;
generated Rust files are not checked in. The client binds the lesser of the
server's advertised version and the XML's maximum version, with minimum required
versions 4 for management (`exit_session`) and 1 for XKB bindings.

When updating, select a tagged release matching the documented versions, copy
the original XML including license notices, review newly introduced messages,
and update the Rust event handlers and compatibility notes. Test binding and
sequence completion against River before committing. Development-branch protocol
additions are outside this baseline.
