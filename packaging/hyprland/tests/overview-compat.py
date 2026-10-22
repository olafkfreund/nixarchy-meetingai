#!/usr/bin/env python3
"""Exercise decorated floating windows and tray lifecycle in a nested compositor.

Uses the selected plugin builds without reloading the live desktop's plugins.
The fixture has multiple hyprbars buttons: a wrong private-object stride must
fail this scenario even when Lennette is absent.
"""
import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--compositor', type=Path, required=True)
parser.add_argument('--plugins', type=Path, required=True)
parser.add_argument('--lennette', type=Path, required=True)
parser.add_argument('--client-dir', type=Path, required=True)
parser.add_argument('--artifacts', type=Path, required=True)
parser.add_argument('--cycles', type=int, default=10)
args = parser.parse_args()
artifacts = args.artifacts.resolve()
artifacts.mkdir(parents=True, exist_ok=True)
checks = []
processes = []
compositor = None
parent_socket = Path(os.environ['WAYLAND_DISPLAY'])
if not parent_socket.is_absolute():
    parent_socket = Path(os.environ['XDG_RUNTIME_DIR']) / parent_socket
runtime = tempfile.TemporaryDirectory(prefix='ln-overview-')
env = dict(os.environ, WAYLAND_DISPLAY=str(parent_socket), XDG_RUNTIME_DIR=runtime.name,
           XDG_CONFIG_HOME=str(artifacts / 'config'),
           XDG_DATA_HOME=str(artifacts / 'data'),
           XDG_CACHE_HOME=str(artifacts / 'cache'), HOME=str(artifacts / 'home'))
for key in ('HYPRLAND_INSTANCE_SIGNATURE', 'DISPLAY', 'LENNETTE_PRIVATE_OVERLAY', 'AQ_BACKEND', 'AQ_DRM_DEVICES'):
    env.pop(key, None)
for directory in ('config/hypr', 'data', 'cache', 'home'):
    (artifacts / directory).mkdir(parents=True, exist_ok=True)
plugins = json.loads(args.plugins.read_text())
config = artifacts / 'hyprland.lua'
config.write_text('''
hl.monitor({output="WAYLAND-1",mode="960x640@60",scale="1"})
hl.config({animations={enabled=false},misc={disable_hyprland_logo=true,disable_splash_rendering=true},ecosystem={no_update_news=true,no_donation_nag=true}})
hl.window_rule({match={class="test.overview.*"},float=true})
''' + ''.join(f'hl.plugin.load({json.dumps(p)})\n' for p in plugins) + '''
hl.config({plugin={hyprbars={bar_height=28,bar_text_size=12}}})
hl.plugin.hyprbars.add_button({bg_color="rgb(ff5555)",fg_color="rgb(ffffff)",size=18,icon="X",action="true"})
hl.plugin.hyprbars.add_button({bg_color="rgb(55ff55)",fg_color="rgb(ffffff)",size=14,icon="+",action="true"})
hl.plugin.hyprbars.add_button({bg_color="rgb(5555ff)",fg_color="rgb(ffffff)",size=12,icon="-",action="true"})
''')

def run(*cmd):
    result = subprocess.run([str(c) for c in cmd], env=env, text=True, capture_output=True, timeout=15)
    assert result.returncode == 0, (cmd, result.stdout, result.stderr)
    return result.stdout.strip()

def ctl(*cmd):
    return run('hyprctl', *cmd)

def lua(code):
    result = ctl('eval', code)
    assert result == 'ok', (code, result)

def clients():
    return json.loads(ctl('-j', 'clients'))

def wait_for(predicate, title, timeout=15):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        assert compositor.poll() is None, 'nested compositor crashed: ' + title
        if predicate():
            return
        time.sleep(.1)
    raise AssertionError('timeout: ' + title)

def spawn(binary, *cmd, name):
    handle = (artifacts / (name + '.log')).open('w')
    process = subprocess.Popen([str(binary), *cmd], env=env, stdout=handle, stderr=subprocess.STDOUT)
    processes.append((process, handle))
    return process

def cycle(label):
    for name, opening, closing in (
        ('scrolloverview', 'hl.dispatch(hl.plugin.scrolloverview.overview("open WAYLAND-1"))', 'hl.dispatch(hl.plugin.scrolloverview.overview("close"))'),
        ('hymission', 'hl.plugin.hymission.open()', 'hl.plugin.hymission.close()'),
    ):
        for _ in range(args.cycles):
            lua(opening)
            time.sleep(.2)
            # Round-trip IPC after several rendered frames detects a crashed process.
            assert json.loads(ctl('-j', 'clients'))
            lua(closing)
            time.sleep(.15)
        checks.append({'test': name, 'phase': label, 'cycles': args.cycles})
        print('PASS', name, label, flush=True)

(artifacts / 'results.json').write_text(json.dumps({'passed': False, 'status': 'running'}))
try:
    executable = args.compositor.resolve()
    watchdog = executable.parent / 'start-hyprland'
    command = [str(watchdog), '--no-nixgl', '--path', str(executable), '--'] if watchdog.exists() else [str(executable)]
    log = (artifacts / 'compositor.log').open('w')
    compositor = subprocess.Popen([*command, '--config', str(config)], env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
    wait_for(lambda: bool(list(Path(runtime.name).glob('hypr/*/.socket.sock'))), 'IPC', 60)
    env['HYPRLAND_INSTANCE_SIGNATURE'] = next(Path(runtime.name).glob('hypr/*/.socket.sock')).parent.name
    wait_for(lambda: bool(list(Path(runtime.name).glob('wayland-*.lock'))), 'Wayland socket')
    env['WAYLAND_DISPLAY'] = next(Path(runtime.name).glob('wayland-*.lock')).name.removesuffix('.lock')
    assert not ctl('configerrors'), ctl('configerrors')
    loaded = json.loads(ctl('-j', 'plugin', 'list'))
    assert sorted(p['path'] for p in loaded) == sorted(plugins)
    for index in range(3):
        process = spawn(args.client_dir.resolve() / 'window', f'test.overview{index}', '206080', name=f'fixture{index}')
        wait_for(lambda: any(w['pid'] == process.pid for w in clients()), 'decorated floating fixture')
    cycle('without Lennette')
    for mode in ('normal', 'private'):
        if mode == 'private':
            env['LENNETTE_PRIVATE_OVERLAY'] = '1'
            rule = Path(__file__).resolve().parents[1] / 'lennette-private.lua'
            lua(f'dofile({json.dumps(str(rule))})')
        started = time.monotonic()
        app = spawn(args.lennette.resolve(), name='lennette-' + mode)
        wait_for(lambda: any(w['pid'] == app.pid for w in clients()), mode + ' app window')
        elapsed = time.monotonic() - started
        print('MAPPED', mode, round(elapsed, 3), 'seconds', flush=True)
        cycle(mode + ' Lennette')
        window = next(w for w in clients() if w['pid'] == app.pid and w['title'] == 'lennette')
        lua('hl.dispatch(hl.dsp.window.close({window=' + json.dumps('address:' + window['address']) + '}))')
        if mode == 'normal':
            wait_for(lambda: any(w['pid'] == app.pid and w['workspace']['name'] == 'special:lennette-hidden' for w in clients()), 'close hides into tray')
            assert app.poll() is None
            assert 'tray icon available' in (artifacts / 'lennette-normal.log').read_text()
            # Launching from a menu again must restore the same instance.
            run(args.lennette.resolve())
            wait_for(lambda: any(w['pid'] == app.pid and w['workspace']['name'] != 'special:lennette-hidden' for w in clients()), 'menu relaunch restores existing app')
            lua('hl.dispatch(hl.dsp.window.close({window=' + json.dumps('address:' + window['address']) + '}))')
            run(args.lennette.resolve(), '--focus-input')
            wait_for(lambda: any(w['pid'] == app.pid and w['workspace']['name'] != 'special:lennette-hidden' for w in clients()), 'shortcut restores existing app')
            assert len([w for w in clients() if w['pid'] == app.pid]) == 1
            app.terminate()
            app.wait(timeout=5)
        else:
            app.wait(timeout=5)
            assert 'tray disabled for private overlay' in (artifacts / 'lennette-private.log').read_text()
        checks.append({'test': mode + ' lifecycle', 'startup_seconds': elapsed})
    (artifacts / 'results.json').write_text(json.dumps({'passed': True, 'checks': checks, 'plugins': plugins}, indent=2))
except Exception as error:
    (artifacts / 'results.json').write_text(json.dumps({'passed': False, 'checks': checks, 'error': str(error)}, indent=2))
    raise
finally:
    for process, handle in processes:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        handle.close()
    if compositor:
        try:
            os.killpg(compositor.pid, signal.SIGTERM)
            compositor.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(compositor.pid, signal.SIGKILL)
            compositor.wait()
        except ProcessLookupError:
            pass
        # The watchdog may still be alive after the compositor exits.
        try:
            os.killpg(compositor.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    runtime.cleanup()
