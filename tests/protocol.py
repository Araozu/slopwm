#!/usr/bin/env python3
# SPDX-License-Identifier: 0BSD
"""Exercise the real binary over a private socket; never touch a live session.

The peer reads request/event schemas from the bundled XML and rejects policy
requests outside manage/render sequences. Only the small core Wayland subset
needed by slopwm is defined here. No third-party Python packages are required.
"""

import array
import os
from pathlib import Path
import select
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import time
import unittest
import xml.etree.ElementTree as ET


ROOT = Path(__file__).resolve().parents[1]
BINARY = str(Path(sys.argv.pop(1)).resolve())
SCHEMAS = {}


def message(name, *args, destructor=False):
    return {"name": name, "args": list(args), "destructor": destructor}


def arg(kind, interface=None):
    return {"type": kind, "interface": interface}


U, I, S, O = arg("uint"), arg("int"), arg("string"), arg("object")
for path in (ROOT / "protocol").glob("*.xml"):
    for interface in ET.parse(path).getroot().findall("interface"):
        schema = {"version": int(interface.get("version"))}
        for kind in ("request", "event"):
            schema[kind] = [
                {"name": item.get("name"), "args": [dict(a.attrib) for a in item.findall("arg")],
                 "destructor": item.get("type") == "destructor"}
                for item in interface.findall(kind)
            ]
        SCHEMAS[interface.get("name")] = schema


def core(name, requests, events=()):
    SCHEMAS[name] = {"version": 4, "request": list(requests), "event": list(events)}


core("wl_display", [message("sync", arg("new_id", "wl_callback")),
                    message("get_registry", arg("new_id", "wl_registry"))],
     [message("error", O, U, S), message("delete_id", U)])
core("wl_registry", [message("bind", U, S, U, arg("new_id"))],
     [message("global", U, S, U), message("global_remove", U)])
core("wl_callback", [], [message("done", U)])
core("wl_compositor", [message("create_surface", arg("new_id", "wl_surface")),
                       message("create_region", arg("new_id", "wl_region"))])
core("wl_shm", [message("create_pool", arg("new_id", "wl_shm_pool"), arg("fd"), I)],
     [message("format", U)])
core("wl_shm_pool", [message("create_buffer", arg("new_id", "wl_buffer"), I, I, I, I, U),
                     message("destroy", destructor=True), message("resize", I)])
core("wl_buffer", [message("destroy", destructor=True)], [message("release")])
core("wl_surface", [message("destroy", destructor=True), message("attach", O, I, I),
                    message("damage", I, I, I, I), message("frame", arg("new_id", "wl_callback")),
                    message("set_opaque_region", O), message("set_input_region", O),
                    message("commit"), message("set_buffer_transform", I),
                    message("set_buffer_scale", I), message("damage_buffer", I, I, I, I),
                    message("offset", I, I)])
core("wl_region", [message("destroy", destructor=True), message("add", I, I, I, I),
                   message("subtract", I, I, I, I)])
core("wl_output", [message("release", destructor=True)],
     [message("geometry", I, I, I, I, I, S, S, I), message("mode", U, I, I, I),
      message("done"), message("scale", I), message("name", S), message("description", S)])


def encode(args, values):
    data = b""
    for spec, value in zip(args, values, strict=True):
        kind = spec["type"]
        if kind in ("uint", "object", "new_id"):
            data += struct.pack("=I", value or 0)
        elif kind in ("int", "fixed"):
            data += struct.pack("=i", value)
        elif kind in ("string", "array"):
            raw = b"" if value is None else (value.encode() + b"\0" if kind == "string" else value)
            data += struct.pack("=I", len(raw)) + raw + bytes((-len(raw)) % 4)
        elif kind != "fd":
            raise AssertionError(kind)
    return data


class Peer:
    def __init__(self, binary=BINARY, versions=(5, 2), layer=True, missing=None,
                 animations="enabled: false"):
        self.temp = tempfile.TemporaryDirectory(prefix="slopwm-protocol-")
        self.config = Path(self.temp.name) / "config.yml"
        self.config.write_text(self.configuration(animations=animations))
        self.log = tempfile.TemporaryFile()
        self.socket, child = socket.socketpair()
        env = dict(os.environ, WAYLAND_SOCKET=str(child.fileno()))
        env.pop("WAYLAND_DEBUG", None)
        self.process = subprocess.Popen([binary, "--config", str(self.config)], env=env,
                                        pass_fds=(child.fileno(),), stdin=subprocess.DEVNULL,
                                        stdout=subprocess.DEVNULL, stderr=self.log)
        child.close()
        self.buffer = b""
        self.fds = []
        self.objects = {1: ("wl_display", 1)}
        self.bound = {}
        self.registry = None
        self.next_id = 0xFF000000
        self.history = []
        self.phase = "idle"
        self.stopped = set()
        self.finished = set()
        self.removed = set()
        self.destroyed = set()
        self.nodes = {}
        self.order = []
        self.positions = {}
        self.proposals = {}
        self.natural = {}
        self.dimensions = {}
        self.visible = {}
        self.focus = None
        self.bindings = {}
        self.enabled = set()
        self.repeats = {}
        self.layers = {}
        self.buffers = set()
        self.manage_count = self.render_count = 0
        self.render_only_count = 0
        self.dirty = False
        self.clips = {}
        self.content_clips = {}
        self.globals = {
            1: ("wl_compositor", 4), 2: ("wl_shm", 1),
            3: ("river_window_manager_v1", versions[0]),
            4: ("river_xkb_bindings_v1", 3),
            5: ("river_input_manager_v1", versions[1]),
            6: ("river_layer_shell_v1", 1),
            10: ("wl_output", 4), 11: ("wl_output", 4),
        }
        if not layer:
            del self.globals[6]
        if missing:
            self.globals = {n: item for n, item in self.globals.items() if item[0] != missing}
        self.wait(lambda: all(name in self.bound for name, _ in self.globals.values()))

    @staticmethod
    def configuration(rate=40, border=2, extra="", animations="enabled: false"):
        actions = ["focus-output-next", "focus-output-previous", "focus-workspace-down",
                   "focus-workspace-up", "move-to-output-next", "move-to-workspace-down",
                   "toggle-fullscreen", "toggle-soft-fullscreen", "preselect-right",
                   "preselect-down", "reload-config", "quit", "close", "focus-next",
                   "focus-previous", "center-window", "move-next", "stack-next", "unstack"]
        return (f"keyboard: {{repeat_rate: {rate}, repeat_delay: 210}}\n"
                f"animations: {{{animations}}}\n"
                f"border: {{width: {border}, color: '#ffffff', unfocused_color: '#808080ff'}}\n"
                "keybindings:\n" + "".join(f"  F{i}: {action}\n" for i, action in enumerate(actions, 1)) + extra)

    def diagnostics(self):
        self.log.seek(0)
        return self.log.read().decode(errors="replace")

    def close(self):
        if self.process.poll() is None:
            self.process.kill()
        self.process.wait(timeout=3)
        self.socket.close()
        for fd in self.fds:
            os.close(fd)
        self.log.close()
        self.temp.cleanup()

    def wait(self, predicate, timeout=3):
        deadline = time.monotonic() + timeout
        while not predicate():
            if time.monotonic() >= deadline:
                raise AssertionError(f"protocol timeout in {self.phase}: {self.history[-8:]}\n{self.diagnostics()}")
            self.read(deadline - time.monotonic())

    def read(self, timeout):
        if not select.select([self.socket], [], [], max(0, timeout))[0]:
            return
        data, controls, _, _ = self.socket.recvmsg(65536, socket.CMSG_SPACE(64 * 4))
        if not data:
            if self.process.poll() is None:
                self.process.wait(timeout=3)
            raise AssertionError(f"unexpected client exit {self.process.returncode}: {self.diagnostics()}")
        for level, kind, raw in controls:
            if level == socket.SOL_SOCKET and kind == socket.SCM_RIGHTS:
                received = array.array("i")
                received.frombytes(raw[:len(raw) - len(raw) % received.itemsize])
                self.fds.extend(received)
        self.buffer += data
        while len(self.buffer) >= 8:
            obj, word = struct.unpack_from("=II", self.buffer)
            length, opcode = word >> 16, word & 0xFFFF
            assert length >= 8
            if len(self.buffer) < length:
                break
            interface, version = self.objects[obj]
            spec = SCHEMAS[interface]["request"][opcode]
            payload = self.buffer[8:length]
            self.buffer = self.buffer[length:]
            values, offset = [], 0
            for a in spec["args"]:
                kind = a["type"]
                if kind == "fd":
                    values.append(self.fds.pop(0))
                    continue
                value = struct.unpack_from("=i" if kind in ("int", "fixed") else "=I", payload, offset)[0]
                offset += 4
                if kind in ("string", "array"):
                    raw = payload[offset:offset + value]
                    offset += value + (-value) % 4
                    value = raw[:-1].decode() if kind == "string" and raw else (None if kind == "string" else raw)
                values.append(value)
                if kind == "new_id" and a.get("interface"):
                    self.objects[value] = (a["interface"], version)
                    self.destroyed.discard(value)
            assert offset == len(payload), (interface, spec, values)
            self.request(obj, interface, spec, values)

    def event(self, obj, name, *values):
        interface = self.objects[obj][0]
        opcode, spec = next((i, item) for i, item in enumerate(SCHEMAS[interface]["event"]) if item["name"] == name)
        payload = encode(spec["args"], values)
        try:
            self.socket.sendall(struct.pack("=II", obj, (8 + len(payload)) << 16 | opcode) + payload)
        except BrokenPipeError:
            if obj != 1 or name != "delete_id":
                raise AssertionError(self.diagnostics()) from None

    def child(self, parent, event, interface):
        obj = self.next_id
        self.next_id += 1
        self.objects[obj] = (interface, self.objects[parent][1])
        self.event(parent, event, obj)
        return obj

    def request(self, obj, interface, spec, values):
        name = spec["name"]
        self.history.append((interface, obj, name, tuple(values), self.phase))
        if interface == "wl_display" and name == "get_registry":
            self.registry = values[0]
            for number, (global_name, version) in self.globals.items():
                self.event(self.registry, "global", number, global_name, version)
        elif interface == "wl_display" and name == "sync":
            self.event(values[0], "done", 1)
            self.event(1, "delete_id", values[0])
        elif interface == "wl_registry" and name == "bind":
            number, global_name, version, new = values
            assert version <= self.globals[number][1]
            self.objects[new] = (global_name, version)
            self.bound[global_name] = new
            self.destroyed.discard(new)
            if global_name == "wl_output" and version >= 4:
                self.event(new, "name", f"TEST-{number}")
            if global_name == "wl_shm":
                self.event(new, "format", 0)
                self.event(new, "format", 1)
        elif name == "stop":
            self.stopped.add(interface)
        elif name == "manage_dirty":
            self.dirty = True
        elif name == "manage_finish":
            assert self.phase == "manage"
            self.manage_count += 1
            self.phase = "wait-render"
        elif name == "render_finish":
            assert self.phase == "render"
            self.render_count += 1
            self.phase = "idle"
            for buffer in self.buffers:
                if buffer not in self.destroyed:
                    self.event(buffer, "release")
            self.buffers.clear()
        elif interface == "river_window_v1":
            if name in {"propose_dimensions", "set_capabilities", "set_tiled", "use_ssd", "fullscreen", "exit_fullscreen",
                        "inform_fullscreen", "inform_not_fullscreen", "inform_maximized", "inform_unmaximized", "close"}:
                assert self.phase == "manage", (name, self.phase)
            elif name != "get_node" and name != "destroy":
                assert self.phase in {"manage", "render"}, (name, self.phase)
            if name == "get_node":
                self.nodes[obj] = values[0]
            elif name == "propose_dimensions":
                self.proposals[obj] = tuple(values)
            elif name in {"show", "hide"}:
                self.visible[obj] = name == "show"
            elif name == "set_clip_box":
                self.clips[obj] = tuple(values)
            elif name == "set_content_clip_box":
                self.content_clips[obj] = tuple(values)
        elif interface == "river_node_v1" and name != "destroy":
            assert self.phase in {"manage", "render"}, (name, self.phase)
            if name == "set_position":
                self.positions[obj] = tuple(values)
            if name in {"place_top", "place_bottom", "place_above", "place_below"}:
                if obj in self.order:
                    self.order.remove(obj)
                if name == "place_top":
                    self.order.append(obj)
                elif name == "place_bottom":
                    self.order.insert(0, obj)
                else:
                    index = self.order.index(values[0]) + (name == "place_above")
                    self.order.insert(index, obj)
        elif interface == "river_shell_surface_v1" and name == "get_node":
            self.nodes[obj] = values[0]
        elif interface == "river_seat_v1" and name in {"focus_window", "clear_focus"}:
            assert self.phase == "manage"
            self.focus = values[0] if values else None
        elif interface == "river_xkb_bindings_v1" and name == "get_xkb_binding":
            seat, new, keysym, _ = values
            self.bindings[(seat, keysym)] = new
        elif interface == "river_xkb_binding_v1" and name in {"enable", "disable"}:
            assert self.phase == "manage"
            (self.enabled.add if name == "enable" else self.enabled.discard)(obj)
        elif interface == "river_input_device_v1" and name == "set_repeat_info":
            assert self.phase == "manage"
            self.repeats[obj] = tuple(values)
        elif interface == "river_layer_shell_v1" and name in {"get_output", "get_seat"}:
            new, target = values
            assert target not in self.layers, "duplicate layer object"
            self.layers[target] = new
        elif interface == "river_layer_shell_output_v1" and name == "set_default":
            assert self.phase == "manage"
        elif interface == "wl_shm" and name == "create_pool":
            os.close(values[1])
        elif interface == "wl_shm_pool" and name == "create_buffer":
            self.buffers.add(values[0])
        if spec["destructor"]:
            if interface in {"river_window_manager_v1", "river_input_manager_v1"}:
                assert interface in self.finished, f"early {interface} destructor"
            elif interface in {"river_window_v1", "river_output_v1", "river_seat_v1"}:
                assert obj in self.removed or "river_window_manager_v1" in self.finished, f"early {interface} destructor"
            self.destroyed.add(obj)
            self.enabled.discard(obj)
            if obj < 0xFF000000:
                self.event(1, "delete_id", obj)

    def setup(self):
        manager = self.bound["river_window_manager_v1"]
        self.outputs = [self.child(manager, "output", "river_output_v1") for _ in range(2)]
        for index, output in enumerate(self.outputs):
            self.event(output, "wl_output", 10 + index)
            self.event(output, "position", index * 1000, 0)
            self.event(output, "dimensions", 1000, 800)
        self.seat = self.child(manager, "seat", "river_seat_v1")
        self.keyboard = self.add_keyboard()
        self.manage()

    def add_keyboard(self):
        keyboard = self.child(self.bound["river_input_manager_v1"], "input_device", "river_input_device_v1")
        self.event(keyboard, "type", 0)
        return keyboard

    def window(self, parent=None, natural=(300, 200)):
        window = self.child(self.bound["river_window_manager_v1"], "window", "river_window_v1")
        self.natural[window] = natural
        self.visible[window] = True
        if parent is not None:
            self.event(window, "parent", parent)
        return window

    def manage(self):
        assert self.phase == "idle"
        self.dirty = False
        old = self.manage_count
        self.phase = "manage"
        self.event(self.bound["river_window_manager_v1"], "manage_start")
        self.wait(lambda: self.manage_count > old)
        for window, size in list(self.proposals.items()):
            actual = self.natural[window] if size == (0, 0) else size
            if self.dimensions.get(window) != actual:
                self.event(window, "dimensions", *actual)
                self.dimensions[window] = actual
        self.proposals.clear()
        self.render()

    def render(self):
        if self.phase == "idle":
            self.render_only_count += 1
        assert self.phase in {"idle", "wait-render"}
        old = self.render_count
        self.phase = "render"
        self.event(self.bound["river_window_manager_v1"], "render_start")
        self.wait(lambda: self.render_count > old)

    def press(self, function):
        self.event(self.bindings[(self.seat, 0xFFBD + function)], "pressed")
        self.manage()

    def click(self, window):
        self.event(self.seat, "window_interaction", window)
        self.manage()

    def closed(self, window):
        self.removed.add(window)
        self.event(window, "closed")
        self.manage()

    def geometry(self, window):
        return self.positions[self.nodes[window]] + self.dimensions[window]

    def animate(self, observe=lambda: None):
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            if not self.dirty:
                self.read(0.03)
            if not self.dirty:
                return
            self.manage()
            observe()
        raise AssertionError("animation failed to settle")

    def shutdown(self, input_first=False, status=0, request=True):
        if request:
            self.process.send_signal(signal.SIGTERM)
        required = {name for name in ("river_window_manager_v1", "river_input_manager_v1") if name in self.bound}
        self.wait(lambda: required <= self.stopped)
        if "river_window_manager_v1" in self.bound and "river_window_manager_v1" not in self.finished:
            self.manage()  # In-flight sequences must still be completed after stop.
        order = ["river_input_manager_v1", "river_window_manager_v1"] if input_first else ["river_window_manager_v1", "river_input_manager_v1"]
        for interface in order:
            if interface not in self.bound or interface in self.finished:
                continue
            self.finished.add(interface)
            self.event(self.bound[interface], "finished")
            self.wait(lambda: self.bound[interface] in self.destroyed)
        self.process.wait(timeout=3)
        assert self.process.returncode == status, self.diagnostics()
        assert self.manage_count + self.render_only_count == self.render_count
        assert "panicked" not in self.diagnostics(), self.diagnostics()


class RegressionTests(unittest.TestCase):
    def peer(self, **kwargs):
        peer = Peer(**kwargs)
        self.addCleanup(peer.close)
        return peer

    def test_animation_progress_retarget_and_idle_without_resize_spam(self):
        p = self.peer(animations="duration_ms: 120, frame_interval_ms: 5", versions=(4, 1))
        p.setup()
        window = p.window()
        p.manage()
        start = len(p.history)
        p.press(16)  # Center the initial column.
        self.assertEqual(p.geometry(window)[:2], (12, 2))
        samples = []
        p.animate(lambda: samples.append(p.geometry(window)[0]))
        self.assertTrue(any(12 < x < 252 for x in samples), samples)
        self.assertEqual(samples, sorted(samples))
        self.assertEqual(p.geometry(window), (252, 2, 496, 796))
        self.assertEqual(p.focus, window)
        self.assertFalse(any(row[2] == "propose_dimensions" for row in p.history[start:]))
        self.assertEqual(len([row for row in p.history[start:] if row[0] == "wl_display" and row[2] == "sync"]), 0)
        after = len(p.history)
        p.read(0.06)
        self.assertFalse(any(row[2] == "manage_dirty" for row in p.history[after:]))

        other = p.window()
        p.manage()
        shown = p.geometry(window)[:2]
        p.press(17)  # Move the new left column past its neighbor mid-animation.
        self.assertEqual(p.geometry(window)[:2], shown)
        p.animate()
        self.assertEqual(p.geometry(other), (492, 2, 496, 796))
        self.assertEqual(p.geometry(window), (-8, 2, 496, 796))
        self.assertEqual(p.focus, other)
        p.shutdown()

    def test_animated_resize_clips_confirmed_content_and_preview_tracks_motion(self):
        p = self.peer(animations="duration_ms: 120, frame_interval_ms: 5")
        p.setup()
        parent = p.window()
        p.manage()
        dialog = p.window(parent)
        p.manage()
        p.press(9)  # Preview attaches to the parent tile.
        p.press(16)
        p.animate()
        self.assertEqual(p.geometry(parent)[:2], (252, 2))
        self.assertEqual(p.geometry(dialog)[:2], (350, 300))
        shell = next(row[3][0] for row in p.history if row[2] == "get_shell_surface")
        self.assertEqual(p.positions[p.nodes[shell]], (500, 0))
        p.click(parent)
        start = len(p.history)
        p.press(8)  # Soft fullscreen requests the final content size once.
        self.assertEqual(p.content_clips[parent][2:], (496, 796))
        widths = []
        p.animate(lambda: widths.append(p.content_clips[parent][2]))
        self.assertTrue(any(496 < width < 976 for width in widths), widths)
        self.assertEqual(p.content_clips[parent][2:], (976, 796))
        self.assertEqual(len([row for row in p.history[start:] if row[1] == parent and row[2] == "propose_dimensions"]), 1)
        # A render-only response that exceeds the proposal remains clipped.
        p.event(parent, "dimensions", 1200, 900)
        p.dimensions[parent] = (1200, 900)
        p.render()
        self.assertEqual(p.content_clips[parent][2:], (976, 796))
        x, y = p.positions[p.nodes[parent]]
        cx, cy, width, height = p.clips[parent]
        self.assertGreaterEqual(x + cx, 0)
        self.assertGreaterEqual(y + cy, 0)
        self.assertLessEqual(x + cx + width, 1000)
        self.assertLessEqual(y + cy + height, 800)
        p.shutdown()

    def test_animation_reload_disable_hide_migrate_and_shutdown(self):
        p = self.peer(animations="duration_ms: 60000, frame_interval_ms: 5")
        p.setup()
        window = p.window()
        p.manage()
        p.press(16)
        p.wait(lambda: p.dirty)
        p.config.write_text(p.configuration(animations="duration_ms: -1"))
        p.process.send_signal(signal.SIGHUP)
        deadline = time.monotonic() + 3
        while "reload failed" not in p.diagnostics() and time.monotonic() < deadline:
            p.read(0.02)
        self.assertIn("reload failed", p.diagnostics())
        p.manage()
        self.assertLess(p.geometry(window)[0], 252)
        # An outstanding frame stays coalesced while the peer delays manage.
        p.wait(lambda: p.dirty)
        after = len(p.history)
        p.render()  # A render-only sequence does not service the pending wakeup.
        p.read(0.04)
        self.assertFalse(any(row[2] == "manage_dirty" for row in p.history[after:]))
        p.config.write_text(p.configuration(animations="enabled: false"))
        p.process.send_signal(signal.SIGHUP)
        deadline = time.monotonic() + 3
        while "configuration reloaded" not in p.diagnostics() and time.monotonic() < deadline:
            p.manage()
            p.read(0.02)
        p.manage()  # Apply the staged replacement at the next manage boundary.
        self.assertEqual(p.geometry(window), (252, 2, 496, 796))
        p.animate()
        self.assertFalse(p.dirty)

        p.config.write_text(p.configuration(animations="duration_ms: 60000, frame_interval_ms: 5"))
        p.press(11)
        p.wait(lambda: p.dirty)
        p.manage()
        other = p.window()
        p.manage()
        p.press(3)
        self.assertFalse(p.visible[window])
        self.assertFalse(p.visible[other])
        p.animate()
        p.press(4)
        self.assertTrue(p.visible[window])
        self.assertTrue(p.visible[other])
        p.press(5)
        self.assertEqual(p.geometry(other)[0], 1012)
        p.press(16)  # Shut down with a transition and timer outstanding.
        p.shutdown(input_first=True)

    def test_nested_dialogs_follow_displayed_parent_at_viewport_edges(self):
        p = self.peer(animations="duration_ms: 120, frame_interval_ms: 5")
        p.setup()
        parent = p.window()
        p.manage()
        p.press(16)
        dialog = p.window(parent)
        p.manage()
        nested = p.window(dialog, (120, 80))
        p.manage()

        def observe():
            self.assertEqual(p.geometry(dialog)[0], p.geometry(parent)[0] + 98)
            self.assertEqual(p.geometry(nested)[0], p.geometry(dialog)[0] + 90)

        observe()
        p.animate(observe)
        p.window()
        p.manage()
        newest = p.window()
        p.manage()
        # The destination is offscreen, but the parent is still being shown.
        self.assertTrue(p.visible[parent])
        self.assertTrue(p.visible[dialog])
        self.assertTrue(p.visible[nested])
        p.animate()
        self.assertFalse(p.visible[parent])
        self.assertFalse(p.visible[dialog])
        self.assertFalse(p.visible[nested])
        p.closed(newest)
        p.animate()
        self.assertTrue(p.visible[parent])
        self.assertTrue(p.visible[dialog])
        self.assertTrue(p.visible[nested])
        p.shutdown()

    def test_zero_duration_and_disabled_animations_apply_immediately(self):
        for settings in ["duration_ms: 0", "enabled: false"]:
            with self.subTest(settings=settings):
                p = self.peer(animations=settings)
                p.setup()
                window = p.window()
                p.manage()
                p.press(16)
                self.assertEqual(p.geometry(window), (252, 2, 496, 796))
                p.animate()
                self.assertFalse(p.dirty)
                p.shutdown()

    def test_stack_animations_and_cancellation_on_lock_fullscreen_output_removal(self):
        p = self.peer(animations="duration_ms: 120, frame_interval_ms: 5")
        p.setup()
        first = p.window()
        p.manage()
        second = p.window()
        p.manage()
        p.animate()
        p.press(18)
        rows = []
        p.animate(lambda: rows.append(p.geometry(second)[1]))
        self.assertTrue(any(2 < y < 402 for y in rows), rows)
        self.assertEqual(p.geometry(first), (12, 2, 496, 396))
        self.assertEqual(p.geometry(second), (12, 402, 496, 396))
        p.press(19)
        p.animate()
        self.assertEqual(p.geometry(second), (12, 2, 496, 796))
        self.assertEqual(p.geometry(first), (512, 2, 496, 796))
        p.press(16)
        manager = p.bound["river_window_manager_v1"]
        p.event(manager, "session_locked")
        p.manage()
        self.assertEqual(p.geometry(second)[:2], (252, 2))
        p.animate()
        self.assertFalse(p.dirty)
        p.event(manager, "session_unlocked")
        p.manage()
        p.press(15)  # Focus the left edge, then enter/leave true fullscreen.
        p.press(7)
        p.animate()
        self.assertFalse(p.dirty)
        p.press(7)
        p.animate()
        self.assertEqual(p.geometry(second)[2:], (496, 796))
        p.window()
        p.manage()
        p.removed.add(p.outputs[0])
        p.event(p.outputs[0], "removed")
        p.manage()
        p.animate()
        self.assertGreaterEqual(p.geometry(second)[0], 1000)
        self.assertGreaterEqual(p.geometry(first)[0], 1000)
        p.shutdown()

    def test_layer_focus_panels_hotplug_and_lock(self):
        p = self.peer()
        p.setup()
        window = p.window()
        p.manage()
        self.assertEqual(p.geometry(window), (12, 2, 496, 796))
        layer = p.layers[p.outputs[0]]
        p.event(layer, "non_exclusive_area", 100, 30, 900, 740)
        p.manage()
        self.assertEqual(p.geometry(window), (12, 32, 496, 736))
        p.press(8)
        self.assertEqual(p.geometry(window), (12, 32, 976, 736))
        p.press(8)
        seat_layer = p.layers[p.seat]
        p.event(seat_layer, "focus_exclusive")
        start = len(p.history)
        p.manage()
        self.assertFalse(any(row[2] == "focus_window" for row in p.history[start:]))
        p.event(seat_layer, "focus_none")
        start = len(p.history)
        p.manage()
        self.assertTrue(any(row[2] == "focus_window" and row[3] == (window,) for row in p.history[start:]))
        p.event(seat_layer, "focus_non_exclusive")
        start = len(p.history)
        p.manage()
        self.assertFalse(any(row[2] == "focus_window" for row in p.history[start:]))
        p.click(window)
        self.assertEqual(p.focus, window)
        manager = p.bound["river_window_manager_v1"]
        p.event(p.bindings[(p.seat, 0xFFC9)], "pressed")  # queued quit, then lock
        p.event(manager, "session_locked")
        p.manage()
        self.assertFalse(p.stopped)
        self.assertFalse(p.enabled)
        p.event(manager, "session_unlocked")
        p.manage()
        self.assertTrue(p.enabled)
        self.assertEqual(p.focus, window)
        old_layer = p.layers[p.outputs[1]]
        p.removed.add(p.outputs[1])
        p.event(p.outputs[1], "removed")
        p.manage()
        self.assertIn(old_layer, p.destroyed)
        replacement = p.child(manager, "output", "river_output_v1")
        p.event(replacement, "position", 1000, 0)
        p.event(replacement, "dimensions", 1000, 800)
        p.manage()
        self.assertIn(replacement, p.layers)
        p.shutdown()

    def test_reload_changes_current_and_hotplugged_keyboards_atomically(self):
        p = self.peer()
        p.setup()
        window = p.window()
        p.manage()
        before = p.geometry(window)
        old_binding = p.bindings[(p.seat, 0xFFC8)]
        p.config.write_text(p.configuration(rate=73, border=4, extra="  F20: close\n"))
        p.press(11)
        p.wait(lambda: any(row[2] == "manage_dirty" for row in p.history))
        p.manage()
        self.assertEqual(p.repeats[p.keyboard], (73, 210))
        self.assertEqual(p.geometry(window), (14, 4, 492, 792))
        self.assertIn(old_binding, p.destroyed)
        self.assertIn((p.seat, 0xFFBD + 20), p.bindings)
        keyboard = p.add_keyboard()
        p.manage()
        self.assertEqual(p.repeats[keyboard], (73, 210))
        p.config.write_text("keyboard: {repeat_rate: -1}\n")
        p.process.send_signal(signal.SIGHUP)
        deadline = time.monotonic() + 3
        while "reload failed" not in p.diagnostics() and time.monotonic() < deadline:
            p.read(0.02)
        self.assertIn("reload failed", p.diagnostics())
        p.manage()
        self.assertEqual(p.repeats[p.keyboard], (73, 210))
        self.assertEqual(p.geometry(window), (14, 4, 492, 792))
        p.config.unlink()
        p.process.send_signal(signal.SIGHUP)
        deadline = time.monotonic() + 3
        while "cannot read" not in p.diagnostics() and time.monotonic() < deadline:
            p.read(0.02)
        self.assertIn("cannot read", p.diagnostics())
        p.config.write_text(p.configuration(rate=0, border=2))
        previous = len(p.history)
        p.process.send_signal(signal.SIGHUP)
        p.wait(lambda: any(row[2] == "manage_dirty" for row in p.history[previous:]))
        p.manage()
        self.assertEqual(p.repeats[keyboard], (0, 210))
        self.assertEqual(p.geometry(window), before)
        p.shutdown(input_first=True)

    def test_dialogs_follow_parent_preserve_scroll_and_restore_focus(self):
        p = self.peer()
        p.setup()
        parent = p.window()
        p.manage()
        before = p.geometry(parent)
        p.press(9)  # Dialogs must not consume pending insertion.
        dialog = p.window(parent)
        p.manage()
        self.assertEqual(p.focus, dialog)
        self.assertEqual(p.geometry(parent), before)
        self.assertEqual(p.geometry(dialog), (110, 300, 300, 200))
        nested = p.window(dialog, (120, 80))
        p.manage()
        self.assertEqual(p.focus, nested)
        p.closed(nested)
        self.assertEqual(p.focus, dialog)
        regular = p.window()
        p.manage()
        self.assertGreater(p.geometry(regular)[0], p.geometry(parent)[0])
        self.assertEqual(p.focus, regular)
        p.click(dialog)
        p.press(5)  # Moving while a dialog is selected moves its parent family.
        self.assertGreaterEqual(p.geometry(parent)[0], 1000)
        self.assertGreaterEqual(p.geometry(dialog)[0], 1000)
        p.window()  # Keep the source workspace occupied when moving the family.
        p.manage()
        p.click(dialog)
        p.press(6)
        self.assertTrue(p.visible[parent])
        self.assertTrue(p.visible[dialog])
        p.press(4)
        self.assertFalse(p.visible[parent])
        self.assertFalse(p.visible[dialog])
        p.press(3)
        p.click(dialog)
        p.closed(dialog)
        self.assertEqual(p.focus, parent)
        orphan = p.window(parent)
        p.manage()
        p.closed(parent)
        self.assertTrue(p.visible[orphan])
        self.assertEqual(p.geometry(orphan)[2:], (496, 796))
        p.shutdown()

    def test_background_dialog_and_render_only_resize(self):
        p = self.peer()
        p.setup()
        parent = p.window()
        p.manage()
        other = p.window()
        p.manage()
        dialog = p.window(parent)
        p.manage()
        self.assertEqual(p.focus, other)
        p.event(dialog, "dimensions", 350, 250)
        p.dimensions[dialog] = (350, 250)
        p.render()
        self.assertEqual(p.geometry(dialog)[2:], (350, 250))
        p.press(3)
        background = p.window(parent)
        p.manage()
        self.assertIsNone(p.focus)
        self.assertFalse(p.visible[background])
        p.press(4)
        p.click(dialog)
        self.assertEqual(p.focus, dialog)
        p.shutdown()

    def test_parent_fullscreen_survives_opening_and_focusing_dialog(self):
        p = self.peer()
        p.setup()
        parent = p.window()
        p.manage()
        p.press(7)
        start = len(p.history)
        dialog = p.window(parent)
        p.manage()
        p.click(dialog)
        self.assertFalse(any(row[1] == parent and row[2] == "exit_fullscreen" for row in p.history[start:]))
        self.assertEqual(p.focus, dialog)
        p.shutdown()

    def test_old_versions_optional_layer_and_quit_action(self):
        p = self.peer(versions=(4, 1), layer=False)
        p.setup()
        self.assertFalse(p.layers)
        self.assertEqual(p.repeats[p.keyboard], (40, 210))
        p.window()
        p.manage()
        p.press(12)
        p.shutdown(request=False)

    def test_dialog_siblings_raise_the_selected_branch_and_late_parents(self):
        p = self.peer()
        p.setup()
        parent = p.window()
        p.manage()
        first = p.window(parent)
        p.manage()
        second = p.window(parent)
        p.manage()
        self.assertLess(p.order.index(p.nodes[first]), p.order.index(p.nodes[second]))
        p.click(first)
        self.assertLess(p.order.index(p.nodes[second]), p.order.index(p.nodes[first]))
        nested = p.window(first)
        p.manage()
        self.assertLess(p.order.index(p.nodes[first]), p.order.index(p.nodes[nested]))
        self.assertLess(p.order.index(p.nodes[second]), p.order.index(p.nodes[first]))
        # Child metadata may refer to a parent announced later in the batch.
        child = p.window()
        later = p.window()
        p.event(child, "parent", later)
        p.manage()
        self.assertEqual(p.geometry(child)[2:], (300, 200))
        self.assertLess(p.order.index(p.nodes[later]), p.order.index(p.nodes[child]))
        p.event(child, "parent", 0)
        p.manage()
        self.assertEqual(p.geometry(child)[2:], (496, 796))
        p.shutdown()

    def test_newer_advertised_versions_are_capped(self):
        p = self.peer(versions=(10, 10))
        p.setup()
        for interface in ("river_window_manager_v1", "river_input_manager_v1"):
            self.assertEqual(p.objects[p.bound[interface]][1], SCHEMAS[interface]["version"])
        p.shutdown()

    def test_missing_input_manager_reports_error_and_stops_cleanly(self):
        p = self.peer(missing="river_input_manager_v1")
        p.shutdown(status=1, request=False)
        self.assertIn("river_input_manager_v1", p.diagnostics())


if __name__ == "__main__":
    unittest.main(verbosity=2)
