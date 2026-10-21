#!/usr/bin/env python3
"""Launch an isolated compositor and compare real protocol exports pixel by pixel."""
import argparse
from collections import Counter
import json
import os
import hashlib
from pathlib import Path
import signal
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[3]
PACKAGE = ROOT / "packaging/hyprland"
parser = argparse.ArgumentParser()
parser.add_argument("--compositor", type=Path, default=ROOT / "target/hyprland-private/Hyprland")
parser.add_argument("--client-dir", type=Path, default=ROOT / "target/hyprland-test-client")
parser.add_argument("--artifacts", type=Path, default=ROOT / "target/hyprland-private-qa")
parser.add_argument("--parent-socket", default=os.environ.get("WAYLAND_DISPLAY"))
parser.add_argument("--drm-node", default=os.environ.get("TEST_DRM_NODE"))
parser.add_argument("--lennette-binary", type=Path)
parser.add_argument("--shortcut-module", type=Path)
parser.add_argument("--doty-config-dir", type=Path)
parser.add_argument("--config-only", action="store_true")
parser.add_argument("--ordinary-launch", action="store_true")
args = parser.parse_args()
if not args.parent_socket:
    parser.error("set WAYLAND_DISPLAY or pass --parent-socket for the parent compositor")
parent_socket = Path(args.parent_socket)
if not parent_socket.is_absolute():
    parent_socket = Path(os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}")) / parent_socket
args.parent_socket = str(parent_socket)
if not args.drm_node:
    args.drm_node = next((str(node) for node in sorted(Path("/dev/dri").glob("renderD*"))
                          if os.access(node, os.R_OK | os.W_OK)), None)
if not args.drm_node:
    parser.error("no accessible DRM render node; pass --drm-node")
artifacts = args.artifacts.resolve()
artifacts.mkdir(parents=True, exist_ok=True)
(artifacts / "results.json").write_text(json.dumps({"passed": False, "status": "running"}) + "\n")
runtime = Path(tempfile.mkdtemp(prefix="ln-qa-", dir="/tmp"))
parent_env = dict(os.environ)
parent_env.setdefault("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}")
parent_env["WAYLAND_DISPLAY"] = args.parent_socket
env = dict(os.environ, XDG_RUNTIME_DIR=str(runtime), XDG_CACHE_HOME=str(artifacts / "cache"),
           WAYLAND_DISPLAY=args.parent_socket, TEST_DRM_NODE=args.drm_node)
env.update(XDG_CONFIG_HOME=str(artifacts / "user-config"), XDG_DATA_HOME=str(artifacts / "user-data"))
(artifacts / "user-config/hypr").mkdir(parents=True, exist_ok=True)
for key in ("HYPRLAND_INSTANCE_SIGNATURE", "DISPLAY", "AQ_BACKEND", "AQ_DRM_DEVICES", "LIBGL_ALWAYS_SOFTWARE"):
    env.pop(key, None)
config = artifacts / "config.lua"
config.write_text('''
hl.monitor({output="WAYLAND-1",mode="960x640@60",position="1920x0",scale="1"})
hl.monitor({output="HEADLESS-1",mode="960x640@60",position="0x0",scale="1"})
hl.monitor({output="HEADLESS-2",mode="960x640@60",position="960x0",scale="1"})
hl.config({animations={enabled=false},misc={disable_hyprland_logo=true,disable_splash_rendering=true,disable_watchdog_warning=true},
    ecosystem={no_update_news=true,no_donation_nag=true},decoration={active_opacity=1,inactive_opacity=1}})
hl.permission(".*", "screencopy", "allow")
hl.permission(".*", "keyboard", "allow")
hl.config({input={resolve_binds_by_sym=false}})
hl.window_rule({match={class="test\\\\.background.*"},float=true,border_size=0,rounding=0})
hl.window_rule({match={class="test\\\\.public"},float=true,pin=true,border_size=0,rounding=0})
''' + ("" if args.ordinary_launch else f'dofile({json.dumps(str(PACKAGE / "lennette-private.lua"))})\n'))
processes = []
checks = []
log = (artifacts / "compositor.log").open("w")
watchdog = args.compositor.resolve().parent / "start-hyprland"
command = ([str(watchdog), "--no-nixgl", "--path", str(args.compositor.resolve()), "--"]
           if watchdog.exists() else [str(args.compositor.resolve())])
compositor = subprocess.Popen([*command, "--config", str(config)], env=env,
                              stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
compositor_pid = compositor.pid

def run(cmd, *, capture=True):
    result = subprocess.run([str(c) for c in cmd], env=env, text=True,
                            stdout=subprocess.PIPE if capture else None, stderr=subprocess.PIPE, timeout=20)
    assert result.returncode == 0, (cmd, result.returncode, result.stdout, result.stderr, compositor.poll())
    return result.stdout

def ctl(*args):
    return run(["hyprctl", *args]).strip()

def lua(code):
    output = ctl("eval", code)
    assert output == "ok", (code, output)

def dispatch(code):
    lua(f"hl.dispatch({code})")

def wait_for(predicate, description, timeout=10):
    until = time.monotonic() + timeout
    while time.monotonic() < until:
        if compositor.poll() is not None:
            raise AssertionError(f"Compositor exited ({compositor.returncode}); see {artifacts}/compositor.log")
        if predicate():
            return
        time.sleep(0.1)
    raise AssertionError(f"Timeout: {description}")

def clients():
    return json.loads(ctl("-j", "clients"))

def bound_host_window():
    def find_window():
        host_clients = json.loads(subprocess.check_output(["hyprctl", "-i", "0", "-j", "clients"], env=parent_env, text=True))
        return next((w for w in host_clients if w.get("pid") == compositor_pid), None)
    wait_for(lambda: find_window() is not None, "nested compositor host window")
    window = find_window()
    selector = f'address:{window["address"]}'
    active = json.loads(subprocess.check_output(["hyprctl", "-i", "0", "-j", "activeworkspace"], env=parent_env, text=True))["id"]
    for action in (f'hl.dsp.window.move({{window="{selector}",workspace="{active}",follow=false}})',
                   f'hl.dsp.window.float({{window="{selector}",action="set"}})',
                   f'hl.dsp.window.resize({{window="{selector}",x=960,y=640}})',
                   f'hl.dsp.window.center({{window="{selector}"}})'):
        result = subprocess.run(["hyprctl", "-i", "0", "eval", f"hl.dispatch({action})"], env=parent_env,
                                capture_output=True, text=True, timeout=20)
        assert result.returncode == 0 and result.stdout.strip() == "ok", (action, result.stdout, result.stderr)
    assert find_window()["floating"], find_window()

def spawn(app_id, color, popup=False):
    handle = (artifacts / f"{app_id}.log").open("w")
    process = subprocess.Popen([str(args.client_dir / "window"), app_id, color] + (["popup"] if popup else []),
                               env=env, stdout=handle, stderr=subprocess.STDOUT)
    processes.append((process, handle))
    wait_for(lambda: any(w["class"] == app_id for w in clients()), f"map {app_id}")
    return process

def ppm(path):
    header, dimensions, depth, pixels = path.read_bytes().split(b"\n", 3)
    assert header == b"P6" and depth == b"255"
    width, height = map(int, dimensions.split())
    assert len(pixels) == width * height * 3
    return width, height, pixels

def capture(name, protocol="wlr", storage="shm", output="HEADLESS-1", cursor=False):
    path = artifacts / f"{name}-{protocol}-{storage}.ppm"
    run([args.client_dir / "capture", protocol, output, storage, path, int(cursor)])
    return ppm(path)

def colors(frame):
    data = frame[2]
    return Counter(zip(data[::3], data[1::3], data[2::3]))

def clean(frame, name, expected=(32, 96, 128)):
    counts = colors(frame)
    # A small conversion tolerance is permitted, but every pixel must match the
    # public scene: a black replacement rectangle or even one magenta pixel fails.
    bad = sum(n for rgb, n in counts.items() if max(abs(a-b) for a, b in zip(rgb, expected)) > 3)
    assert bad == 0, (name, bad, counts.most_common(8))
    checks.append({"test": name, "pixels": frame[0] * frame[1], "unexpected_pixels": bad})
    print(f"PASS {name}: {frame[0]}x{frame[1]}, no overlay or replacement pixels", flush=True)

def move(x, y, app_id="dev.lennette.overlay"):
    monitor = "HEADLESS-1" if x < 960 else "HEADLESS-2"
    dispatch(f'hl.dsp.window.move({{window="class:{app_id}",monitor="{monitor}",follow=false}})')
    dispatch(f'hl.dsp.window.move({{window="class:{app_id}",x={x},y={y}}})')
    time.sleep(0.07)

def check_doty_config():
    if not args.doty_config_dir:
        return
    lua(f'package.path = {json.dumps(str(args.doty_config_dir.resolve() / "?.lua"))} .. ";" .. package.path')
    for module in ("core.input", "binds", "decorations", "layout", "misc", "windowrules", "workspace.rules"):
        lua(f'require("modules.{module}")')
    assert not ctl("configerrors"), ctl("configerrors")
    checks.append({"test": "doty input, bindings, decorations, layout, misc, window and workspace rules load without plugins"})

try:
    wait_for(lambda: bool(list((runtime / "hypr").glob("*/.socket.sock"))), "compositor IPC", 60)
    signature = next((runtime / "hypr").glob("*/.socket.sock")).parent.name
    env["HYPRLAND_INSTANCE_SIGNATURE"] = signature
    # IPC can appear before Hyprland finishes writing its instance metadata.
    def initialized_instance():
        try:
            return next((i for i in json.loads(ctl("instances", "-j")) if i["instance"] == signature), None)
        except (ValueError, AssertionError):
            return None
    wait_for(lambda: initialized_instance() is not None, "compositor instance metadata", 60)
    compositor_pid = initialized_instance()["pid"]
    wait_for(lambda: bool(list(runtime.glob("wayland-*"))), "Wayland socket")
    env["WAYLAND_DISPLAY"] = next(p.name for p in runtime.glob("wayland-*") if not p.name.endswith(".lock"))
    bound_host_window()
    assert not ctl("configerrors"), ctl("configerrors")
    for output in ("HEADLESS-1", "HEADLESS-2"):
        assert ctl("output", "create", "headless", output) == "ok"
    if args.config_only:
        assert args.doty_config_dir, "--config-only requires --doty-config-dir"
        check_doty_config()
        (artifacts / "results.json").write_text(json.dumps({"passed": True, "checks": checks}, indent=2) + "\n")
        print("PASS doty configuration modules", flush=True)
        raise SystemExit(0)
    if args.ordinary_launch:
        assert args.lennette_binary, "--ordinary-launch requires --lennette-binary"
        handle = (artifacts / "lennette.log").open("w")
        app = subprocess.Popen([str(args.lennette_binary.resolve())], env=env, stdout=handle, stderr=subprocess.STDOUT)
        processes.append((app, handle))
        wait_for(lambda: any(w["class"] == "dev.lennette.overlay" for w in clients()), "ordinary first-launch window", 60)
        time.sleep(1)
        window = next(w for w in clients() if w["class"] == "dev.lennette.overlay")
        assert window["floating"] and window["pinned"], window
        assert window["size"] == [720, 620], window
        (artifacts / "results.json").write_text(json.dumps({"passed": True, "test": "ordinary first launch floats and pins without preloaded app rules", "window": window}, indent=2) + "\n")
        print("PASS ordinary first launch floats and pins without preloaded app rules", flush=True)
        raise SystemExit(0)
    dispatch('hl.dsp.focus({monitor="HEADLESS-1"})')
    spawn("test.background", "206080")
    dispatch('hl.dsp.window.move({window="class:test.background",monitor="HEADLESS-1",follow=false})')
    dispatch('hl.dsp.window.fullscreen({window="class:test.background",action="set"})')
    dispatch('hl.dsp.focus({monitor="HEADLESS-2"})')
    spawn("test.background2", "206080")
    dispatch('hl.dsp.window.move({window="class:test.background2",monitor="HEADLESS-2",follow=false})')
    dispatch('hl.dsp.window.fullscreen({window="class:test.background2",action="set"})')
    dispatch('hl.dsp.focus({monitor="HEADLESS-1"})')
    time.sleep(0.5)
    (artifacts / "initial-clients.json").write_text(json.dumps(clients(), indent=2))
    clean(capture("baseline"), "baseline")
    overlay = spawn("dev.lennette.overlay", "ff00ff", popup=True)
    move(250, 180)
    state = next(w for w in clients() if w["class"] == "dev.lennette.overlay")
    assert state["floating"] and state["pinned"], state
    (artifacts / "protected-fixture.json").write_text(json.dumps(state, indent=2))
    assert run([PACKAGE / "launch.sh", "--check"]).strip().endswith("private_overlay rule.")
    for protocol in ("wlr", "ext"):
        for storage in ("shm", "dma"):
            clean(capture("overlay-and-popup", protocol, storage), f"{protocol}/{storage} overlay and popup")
    # Direct window capture must not reveal the protected window either.
    assert colors(capture("direct-window", "window", "shm", "dev.lennette.overlay"))[(255, 0, 255)] == 0
    checks.append({"test": "direct protected window capture denied"})
    # The outer compositor sees the nested compositor as one ordinary window.
    # This provides an independent view of the LOCAL display, including overlays.
    lua('hl.monitor({output="WAYLAND-1",mirror="HEADLESS-1",scale="1"})')
    time.sleep(0.7)
    def local_display(name):
        host_clients = json.loads(subprocess.check_output(["hyprctl", "-i", "0", "-j", "clients"], env=parent_env, text=True))
        windows = [w for w in host_clients if w.get("pid") == compositor_pid]
        assert len(windows) == 1, [(w.get("class"), w.get("pid")) for w in host_clients]
        window = windows[0]
        focus = f'hl.dispatch(hl.dsp.focus({{window="address:{window["address"]}"}}))'
        subprocess.run(["hyprctl", "-i", "0", "eval", focus], env=parent_env, check=True,
                       capture_output=True, text=True, timeout=20)
        time.sleep(0.5)
        x, y = window["at"]; width, height = window["size"]
        path = artifacts / f"{name}-local-display.ppm"
        result = subprocess.run(["grim", "-t", "ppm", "-g", f"{x},{y} {width}x{height}", str(path)],
                                env=parent_env, capture_output=True, text=True, timeout=20)
        assert result.returncode == 0, result.stderr
        visible = colors(ppm(path))[(255, 0, 255)]
        if visible <= 1000:
            (artifacts / "local-visibility-state.json").write_text(json.dumps({"host": window, "nested_clients": clients(), "nested_monitors": json.loads(ctl("-j", "monitors"))}, indent=2))
        assert visible > 1000, (name, visible)
        checks.append({"test": name, "local_overlay_pixels": visible})
        print(f"PASS {name}: {visible} overlay pixels visible locally", flush=True)
    local_display("local visibility over fullscreen")
    dispatch('hl.dsp.focus({monitor="HEADLESS-1"})')
    public = spawn("test.public", "00ff00")
    move(250, 180, "test.public")
    local_display("local visibility over competing pinned window")
    exported = colors(capture("competing-pinned"))
    assert exported[(255, 0, 255)] == 0 and exported[(0, 255, 0)] > 1000, exported.most_common(5)
    checks.append({"test": "public pinned window is exported beneath private overlay"})
    dispatch('hl.dsp.focus({monitor="HEADLESS-1"})')
    move(250, 180)
    move(250, 180, "test.public")
    dispatch('hl.dsp.cursor.move({x=300,y=200})')
    run(["wlrctl", "pointer", "click"])
    wait_for(lambda: json.loads(ctl("-j", "activewindow")).get("class") == "dev.lennette.overlay",
             "pointer focuses private overlay above public pinned window")
    checks.append({"test": "pointer hit testing prioritizes private overlay"})
    public.terminate(); public.wait(timeout=5)
    # Focusing a pinned window can select its original workspace. Restore the
    # public fullscreen scenes before comparing against their known pixels.
    dispatch('hl.dsp.focus({window="class:test.background2"})')
    dispatch('hl.dsp.focus({window="class:test.background$"})')

    for index, (x, y) in enumerate([(0, 0), (200, 250), (800, 120), (930, 200), (1000, 70), (1250, 300)]):
        move(x, y)
        for output in ("HEADLESS-1", "HEADLESS-2"):
            clean(capture(f"move-{index}-{output}", output=output), f"move {index}, {output}")
    # Capture under output rotation and fractional scaling.
    lua('hl.monitor({output="HEADLESS-2",mode="800x600@60",position="960x0",scale="1.25",transform=1})')
    time.sleep(0.5)
    move(1000, 100)
    clean(capture("rotated-scaled", "ext", "dma", "HEADLESS-2"), "rotation + fractional scale + DMA-BUF")
    # A pinned overlay survives workspace changes.
    dispatch('hl.dsp.focus({monitor="HEADLESS-2"})')
    previous_workspace = next(m for m in json.loads(ctl("-j", "monitors")) if m["name"] == "HEADLESS-2")["activeWorkspace"]["id"]
    dispatch('hl.dsp.focus({workspace="8"})')
    state = next(w for w in clients() if w["class"] == "dev.lennette.overlay")
    assert state["pinned"], state
    dispatch(f'hl.dsp.focus({{workspace="{previous_workspace}"}})')
    # Positive control proves the marker detector sees the fixture if privacy is disabled.
    dispatch('hl.dsp.window.set_prop({window="class:dev.lennette.overlay",prop="private_overlay",value="0"})')
    visible = colors(capture("positive-control", output="HEADLESS-2"))[(255, 0, 255)]
    assert visible > 1000, visible
    dispatch('hl.dsp.window.set_prop({window="class:dev.lennette.overlay",prop="private_overlay",value="1"})')
    clean(capture("restored", output="HEADLESS-2"), "privacy restored")
    # Retain exclusion after the source objects disappear and snapshots animate.
    lua('hl.config({animations={enabled=true}})')
    overlay.terminate()
    overlay.wait(timeout=5)
    for index in range(8):
        clean(capture(f"closing-{index}", output="HEADLESS-2"), f"closing animation {index}")
    checks.append({"test": "floating, pinned, workspace switching and positive control", "visible_control_pixels": visible})
    if args.lennette_binary:
        dispatch('hl.dsp.focus({monitor="HEADLESS-1"})')
        if args.shortcut_module:
            # Match Home Manager ownership: the app must not replace these
            # bindings with its ordinary, unprotected launcher commands.
            shortcut_file = artifacts / "user-config/hypr/lennette.lua"
            if shortcut_file.exists(): shortcut_file.chmod(0o644)
            shortcut_file.write_text(args.shortcut_module.read_text())
            shortcut_file.chmod(0o444)
            lua(f'dofile({json.dumps(str(shortcut_file))})')
        application_log = (artifacts / "lennette.log").open("w")
        app_env = dict(env, LENNETTE_BINARY=str(args.lennette_binary.resolve()))
        application = subprocess.Popen([str(PACKAGE / "launch.sh")], env=app_env,
                                       stdout=application_log, stderr=subprocess.STDOUT)
        processes.append((application, application_log))
        wait_for(lambda: any(w["class"] == "dev.lennette.overlay" for w in clients()), "real Lennette window", 30)
        move(250, 180)
        clean(capture("real-lennette"), "real Lennette overlay excluded")
        dispatch('hl.dsp.window.set_prop({window="class:dev.lennette.overlay",prop="private_overlay",value="0"})')
        actual_frame = capture("real-lennette-positive-control")
        different = sum(n for rgb, n in colors(actual_frame).items() if max(abs(a-b) for a,b in zip(rgb,(32,96,128))) > 3)
        assert different > 1000, different
        dispatch('hl.dsp.window.set_prop({window="class:dev.lennette.overlay",prop="private_overlay",value="1"})')
        clean(capture("real-lennette-restored"), "real Lennette protection restored")
        checks.append({"test": "real Lennette positive control", "visible_control_pixels": different})
        assert "tray disabled for private overlay" in (artifacts / "lennette.log").read_text()
        checks.append({"test": "real Lennette suppresses its tray in private mode"})
        if args.shortcut_module:
            bindings = json.loads(ctl("-j", "binds"))
            expected_keys = {"D", "backslash", "I", "M", "A", "S"}
            # Lua dispatchers expose closure IDs, rather than their shell text,
            # through IPC. Verify keys plus the immutable source module.
            def binding_key(binding):
                return "backslash" if binding.get("keycode") == 51 else binding.get("key")
            protected = [b for b in bindings if b.get("modmask") == 5 and binding_key(b) in expected_keys]
            assert len(protected) == 6 and {binding_key(b) for b in protected} == expected_keys, bindings
            assert all(b["dispatcher"] == "HL.Dispatcher(exec_cmd)" for b in protected), protected
            assert shortcut_file.read_text() == args.shortcut_module.read_text()
            checks.append({"test": "all six protected compositor bindings installed"})
            keyboard_log = (artifacts / "keyboard.log").open("w")
            keyboard = subprocess.Popen([str(args.client_dir / "keyboard")], env=env, stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=keyboard_log, text=True)
            processes.append((keyboard, keyboard_log))
            assert keyboard.stdout.readline().strip() == "ready"
            def press(code):
                keyboard.stdin.write(f"{code}\n")
                keyboard.stdin.flush()
                assert keyboard.stdout.readline().strip() == "sent"
            def app_window():
                return next(w for w in clients() if w.get("pid") == application.pid and w["title"] == "lennette")
            press(43)
            wait_for(lambda: app_window()["workspace"]["name"] == "special:lennette-hidden", "physical shortcut hides overlay")
            clean(capture("shortcut-hidden"), "hidden overlay stays excluded")
            press(43)
            wait_for(lambda: app_window()["workspace"]["name"] != "special:lennette-hidden" and app_window()["pinned"], "physical shortcut restores pinned overlay")
            clean(capture("shortcut-restored"), "restored overlay stays excluded")
            press(43)
            press(23)
            wait_for(lambda: app_window()["workspace"]["name"] != "special:lennette-hidden" and app_window()["pinned"], "input shortcut restores hidden overlay")
            clean(capture("shortcut-input"), "input shortcut stays excluded")
            count = sum(w.get("pid") == application.pid for w in clients())
            press(32)
            wait_for(lambda: sum(w.get("pid") == application.pid for w in clients()) > count,
                     "real dashboard shortcut opens protected window", 30)
            clean(capture("dashboard-shortcut"), "real dashboard shortcut stays excluded")
            for w in clients():
                if w.get("pid") == application.pid:
                    assert w["class"] in ("dev.lennette.overlay", "lennette") and w["floating"] and w["pinned"], w

    check_doty_config()

    evidence = {"passed": True, "checks": checks,
                "compositor": str(args.compositor.resolve()),
                "patch_sha256": hashlib.sha256((PACKAGE / "private-overlay.patch").read_bytes()).hexdigest()}
    if args.lennette_binary:
        evidence["lennette_sha256"] = hashlib.sha256(args.lennette_binary.read_bytes()).hexdigest()
    (artifacts / "results.json").write_text(json.dumps(evidence, indent=2) + "\n")
    print(f"PASS {len(checks)} checks. Artifacts: {artifacts}", flush=True)
except Exception as error:
    (artifacts / "results.json").write_text(json.dumps({"passed": False, "checks": checks, "error": str(error)}, indent=2) + "\n")
    raise
finally:
    for process, handle in processes:
        if process.poll() is None:
            process.terminate()
            try: process.wait(timeout=3)
            except subprocess.TimeoutExpired: process.kill(); process.wait()
        handle.close()
    if compositor.poll() is None:
        os.killpg(compositor.pid, signal.SIGTERM)
        try: compositor.wait(timeout=5)
        except subprocess.TimeoutExpired: os.killpg(compositor.pid, signal.SIGKILL); compositor.wait()
    # Hyprland's watchdog may outlive the main process after shutdown.
    try: os.killpg(compositor.pid, signal.SIGKILL)
    except ProcessLookupError: pass
    log.close()
    # Keep compositor logs in artifacts, not in a temporary runtime directory.
    for path in (runtime / "hypr").glob("*/hyprland*.log"):
        (artifacts / "hyprland.log").write_bytes(path.read_bytes())
    import shutil
    shutil.rmtree(runtime)
