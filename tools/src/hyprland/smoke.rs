use super::*;

const CONFIG: &str = r#"
hl.monitor({output="WAYLAND-1",mode="960x640@60",position="1920x0",scale="1"})
hl.monitor({output="HEADLESS-1",mode="960x640@60",position="0x0",scale="1"})
hl.monitor({output="HEADLESS-2",mode="960x640@60",position="960x0",scale="1"})
hl.config({animations={enabled=false},misc={disable_hyprland_logo=true,disable_splash_rendering=true,disable_watchdog_warning=true},
    ecosystem={no_update_news=true,no_donation_nag=true},decoration={active_opacity=1,inactive_opacity=1}})
hl.permission(".*", "screencopy", "allow")
hl.permission(".*", "keyboard", "allow")
hl.config({input={resolve_binds_by_sym=false}})
hl.window_rule({match={class="test\\.background.*"},float=true,border_size=0,rounding=0})
hl.window_rule({match={class="test\\.public"},float=true,pin=true,border_size=0,rounding=0})
"#;

fn drm_node(requested: Option<String>) -> Result<String> {
    if let Some(node) = requested.or_else(|| std::env::var("TEST_DRM_NODE").ok()) {
        return Ok(node);
    }
    let mut nodes: Vec<_> = fs::read_dir("/dev/dri")?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("renderD"))
        .map(|e| e.path())
        .collect();
    nodes.sort();
    nodes
        .into_iter()
        .find(|p| {
            fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(p)
                .is_ok()
        })
        .map(|p| p.to_string_lossy().into_owned())
        .context("No accessible DRM render node; pass --drm-node")
}
fn move_window(h: &Harness, x: i32, y: i32, class: &str) -> Result<()> {
    let monitor = if x < 960 { "HEADLESS-1" } else { "HEADLESS-2" };
    let selector = quote(format!("class:{class}"));
    h.dispatch(&format!(
        "hl.dsp.window.move({{window={selector},monitor={},follow=false}})",
        quote(monitor)
    ))?;
    h.dispatch(&format!(
        "hl.dsp.window.move({{window={selector},x={x},y={y}}})"
    ))?;
    pause(70);
    Ok(())
}
fn clean(
    h: &mut Harness,
    clients: &Path,
    name: &str,
    protocol: &str,
    storage: &str,
    output: &str,
) -> Result<()> {
    h.clean(h.capture(clients, name, protocol, storage, output)?, name)
}
fn doty(h: &mut Harness, config: Option<&Path>, plugin_paths: &[String]) -> Result<()> {
    let Some(config) = config else {
        return Ok(());
    };
    h.lua(&format!(
        "package.path = {} .. \";\" .. package.path",
        path_quote(&absolute(config)?.join("?.lua"))
    ))?;
    for module in [
        "core.input",
        "binds",
        "decorations",
        "layout",
        "misc",
        "windowrules",
        "workspace.rules",
    ] {
        h.lua(&format!("require(\"modules.{module}\")"))?;
    }
    ensure!(
        h.ctl(&["configerrors"])?.is_empty(),
        "Doty configuration has errors"
    );
    h.checks.push(json!({"test":"doty input, bindings, decorations, layout, misc, window and workspace rules load","plugins":plugin_paths}));
    Ok(())
}
fn protected_bindings(bindings: &Value) -> bool {
    let expected: BTreeSet<_> = ["D", "backslash", "I", "M", "A", "S"].into_iter().collect();
    let Some(bindings) = bindings.as_array() else {
        return false;
    };
    let selected: Vec<_> = bindings
        .iter()
        .filter_map(|b| {
            let key = if b["keycode"] == 51 {
                "backslash"
            } else {
                b["key"].as_str().unwrap_or("")
            };
            (b["modmask"] == 5 && expected.contains(key)).then_some((key, b))
        })
        .collect();
    selected.len() == 6
        && selected
            .iter()
            .map(|(key, _)| *key)
            .collect::<BTreeSet<_>>()
            == expected
        && selected
            .iter()
            .all(|(_, b)| b["dispatcher"] == "HL.Dispatcher(exec_cmd)")
}
fn keyboard(h: &mut Harness, clients: &Path, app_pid: u32) -> Result<()> {
    let log = fs::File::create(h.artifacts.join("keyboard.log"))?;
    let mut cmd = command(&h.env, clients.join("keyboard"));
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(log);
    let mut process = Managed::spawn(&mut cmd)?;
    let mut input = process.0.stdin.take().context("Keyboard stdin missing")?;
    let mut output = process.0.stdout.take().context("Keyboard stdout missing")?;
    h.processes.push(process);
    ensure!(
        read_ack(&mut output)? == "ready",
        "Keyboard did not initialize"
    );
    let mut press = |code: u32| -> Result<()> {
        writeln!(input, "{code}")?;
        input.flush()?;
        ensure!(
            read_ack(&mut output)? == "sent",
            "Keyboard did not send key"
        );
        Ok(())
    };
    press(43)?;
    h.wait("physical shortcut hides overlay", 15, |h| {
        Ok(h.app_window(app_pid)?["workspace"]["name"] == "special:lennette-hidden")
    })?;
    clean(h, clients, "shortcut-hidden", "wlr", "shm", "HEADLESS-1")?;
    press(23)?;
    press(32)?;
    pause(500);
    ensure!(
        h.app_window(app_pid)?["workspace"]["name"] == "special:lennette-hidden",
        "Hidden input shortcut restored overlay"
    );
    ensure!(
        h.clients()?.iter().filter(|w| w["pid"] == app_pid).count() == 1,
        "Hidden dashboard shortcut created a window"
    );
    h.checks
        .push(json!({"test":"hidden input/dashboard shortcuts are blocked"}));
    press(43)?;
    h.wait("physical shortcut restores pinned overlay", 15, |h| {
        let w = h.app_window(app_pid)?;
        Ok(w["workspace"]["name"] != "special:lennette-hidden" && w["pinned"] == true)
    })?;
    clean(h, clients, "shortcut-restored", "wlr", "shm", "HEADLESS-1")?;
    let count = h.clients()?.iter().filter(|w| w["pid"] == app_pid).count();
    press(32)?;
    h.wait("real dashboard shortcut", 30, |h| {
        Ok(h.clients()?.iter().filter(|w| w["pid"] == app_pid).count() > count)
    })?;
    clean(h, clients, "dashboard-shortcut", "wlr", "shm", "HEADLESS-1")?;
    for w in h.clients()?.iter().filter(|w| w["pid"] == app_pid) {
        ensure!(
            (w["class"] == "dev.lennette.overlay" || w["class"] == "lennette")
                && w["floating"] == true
                && w["pinned"] == true,
            "Unprotected app window: {w}"
        );
    }
    Ok(())
}
fn overview_checks(h: &mut Harness, clients: &Path) -> Result<()> {
    for (name, opening, closing) in super::overview::OVERVIEWS {
        h.dispatch("hl.dsp.cursor.move({x=800,y=500})")?;
        h.dispatch("hl.dsp.focus({monitor=\"HEADLESS-1\"})")?;
        if name == "scrolloverview" {
            h.lua("lennette_qa_overview_clicked=false; hl.define_submap(\"scrolloverview\", function() hl.bind(\"mouse:272\", function() lennette_qa_overview_clicked=true end) end)")?;
        } else {
            h.lua("hl.config({plugin={hymission={vim_keys=true}}})")?;
        }
        h.lua(&opening.replace("WAYLAND-1", "HEADLESS-1"))?;
        pause(500);
        if name == "hymission" {
            ensure!(
                h.json(&["hymission-overview-state"])?["active"] == true,
                "Hymission did not open"
            );
        }
        for protocol in ["wlr", "ext"] {
            for storage in ["shm", "dma"] {
                clean(
                    h,
                    clients,
                    &format!("{name} active, {protocol}/{storage}"),
                    protocol,
                    storage,
                    "HEADLESS-1",
                )?;
            }
        }
        if name == "scrolloverview" {
            ensure!(
                h.ctl(&["submap"])? == "scrolloverview",
                "Wrong overview submap"
            );
            h.dispatch("hl.dsp.cursor.move({x=800,y=500})")?;
            h.call("wlrctl", &["pointer", "click"])?;
            save(
                &h.artifacts.join("overview-input-state.json"),
                &json!({"binds":h.json(&["-j","binds"])?,"submap":h.ctl(&["submap"])?,"cursor":h.json(&["-j","cursorpos"])?}),
            )?;
            h.wait("overview callback", 10, |h| {
                Ok(h.lua("assert(lennette_qa_overview_clicked)").is_ok())
            })?;
            h.checks.push(
                json!({"test":"scroll overview mouse submap invokes the current bind callback"}),
            );
        }
        h.lua(closing)?;
        pause(500);
        clean(
            h,
            clients,
            &format!("{name} closed"),
            "wlr",
            "shm",
            "HEADLESS-1",
        )?;
    }
    Ok(())
}

pub fn run(root: &Path, args: &[String]) -> Result<()> {
    if args.iter().any(|s| s == "--help") {
        println!(
            "cargo xtask hyprland-smoke [--compositor PATH] [--client-dir PATH] [--artifacts DIR] [--parent-socket PATH] [--drm-node PATH] [--lennette-binary PATH] [--shortcut-module PATH] [--doty-config-dir DIR] [--plugins JSON] [--config-only] [--ordinary-launch]"
        );
        return Ok(());
    }
    let mut args = Arguments::parse(args, &["--config-only", "--ordinary-launch"])?;
    let compositor = absolute(&args.path(
        "--compositor",
        root.join("target/hyprland-private/Hyprland"),
    ))?;
    let clients = absolute(&args.path("--client-dir", root.join("target/hyprland-test-client")))?;
    let artifacts = args.path("--artifacts", root.join("target/hyprland-private-qa"));
    let parent = args.take("--parent-socket");
    let drm = drm_node(args.take("--drm-node"))?;
    let binary = args
        .take("--lennette-binary")
        .map(PathBuf::from)
        .map(|p| absolute(&p))
        .transpose()?;
    let shortcuts = args.take("--shortcut-module").map(PathBuf::from);
    let doty_path = args.take("--doty-config-dir").map(PathBuf::from);
    let plugin_paths = plugins(args.take("--plugins").map(PathBuf::from))?;
    let config_only = args.flag("--config-only");
    let ordinary = args.flag("--ordinary-launch");
    args.finish()?;
    ensure!(
        !config_only || doty_path.is_some(),
        "--config-only requires --doty-config-dir"
    );
    ensure!(
        !ordinary || binary.is_some(),
        "--ordinary-launch requires --lennette-binary"
    );
    let mut h = Harness::new(&artifacts, parent, false)?;
    h.env.insert("TEST_DRM_NODE".into(), drm.clone().into());
    // Compositor bindings inherit its startup environment, not later changes
    // made only for the app process. Point test launchers at this exact binary.
    if let Some(binary) = &binary {
        h.env
            .insert("LENNETTE_BINARY".into(), binary.as_os_str().into());
    }
    let mut config = CONFIG.to_string();
    if !ordinary {
        config.push_str(&format!(
            "dofile({})\n",
            path_quote(&root.join("packaging/hyprland/lennette-private.lua"))
        ));
    }
    for plugin in &plugin_paths {
        config.push_str(&format!("hl.plugin.load({})\n", quote(plugin)));
    }
    if !plugin_paths.is_empty() {
        h.env.insert(
            "LENNETTE_ALLOWED_PLUGIN_PATHS".into(),
            serde_json::to_string(&plugin_paths)?.into(),
        );
    }
    let mut loaded_plugins = json!([]);
    let result = (|| -> Result<()> {
        h.start(&compositor, &config)?;
        h.bound_host()?;
        loaded_plugins = h.json(&["-j", "plugin", "list"])?;
        if !plugin_paths.is_empty() {
            let loaded = loaded_plugins.as_array().context("Invalid plugin list")?;
            let actual: BTreeSet<_> = loaded.iter().filter_map(|p| p["path"].as_str()).collect();
            ensure!(
                actual == plugin_paths.iter().map(String::as_str).collect()
                    && loaded.iter().all(|p| p["privateCaptureCompatible"] == true),
                "Incompatible/missing plugin builds: {loaded_plugins}"
            );
            h.checks.push(
                json!({"test":"all configured plugins loaded together","plugins":loaded_plugins}),
            );
        }
        for output in ["HEADLESS-1", "HEADLESS-2"] {
            ensure!(
                h.ctl(&["output", "create", "headless", output])? == "ok",
                "Cannot create headless output"
            );
        }
        if config_only {
            return doty(&mut h, doty_path.as_deref(), &plugin_paths);
        }
        if ordinary {
            h.spawn(binary.as_ref().unwrap(), &[], "lennette")?;
            h.wait("ordinary first-launch window", 60, |h| {
                Ok(h.clients()?
                    .iter()
                    .any(|w| w["class"] == "dev.lennette.overlay"))
            })?;
            pause(1000);
            let w = h.window("dev.lennette.overlay")?;
            ensure!(
                w["floating"] == true && w["pinned"] == true && w["size"] == json!([720, 620]),
                "Ordinary launch is not floating/pinned with expected size: {w}"
            );
            h.checks.push(json!({"test":"ordinary first launch floats and pins without preloaded app rules","window":w}));
            return Ok(());
        }
        for (output, class) in [
            ("HEADLESS-1", "test.background"),
            ("HEADLESS-2", "test.background2"),
        ] {
            h.dispatch(&format!("hl.dsp.focus({{monitor={}}})", quote(output)))?;
            h.fixture(&clients, class, "206080", false)?;
            h.dispatch(&format!(
                "hl.dsp.window.move({{window={},monitor={},follow=false}})",
                quote(format!("class:{class}")),
                quote(output)
            ))?;
            h.dispatch(&format!(
                "hl.dsp.window.fullscreen({{window={},action=\"set\"}})",
                quote(format!("class:{class}"))
            ))?;
        }
        h.dispatch("hl.dsp.focus({monitor=\"HEADLESS-1\"})")?;
        pause(500);
        save(
            &h.artifacts.join("initial-clients.json"),
            &json!(h.clients()?),
        )?;
        clean(&mut h, &clients, "baseline", "wlr", "shm", "HEADLESS-1")?;
        let overlay = h.fixture(&clients, "dev.lennette.overlay", "ff00ff", true)?;
        move_window(&h, 250, 180, "dev.lennette.overlay")?;
        let state = h.window("dev.lennette.overlay")?;
        ensure!(
            state["floating"] == true && state["pinned"] == true,
            "Fixture not floating/pinned: {state}"
        );
        save(&h.artifacts.join("protected-fixture.json"), &state)?;
        ensure!(
            h.call(root.join("packaging/hyprland/launch.sh"), &["--check"])?
                .ends_with("private_overlay rule."),
            "Private launcher validation failed"
        );
        for protocol in ["wlr", "ext"] {
            for storage in ["shm", "dma"] {
                clean(
                    &mut h,
                    &clients,
                    &format!("{protocol}/{storage} overlay and popup"),
                    protocol,
                    storage,
                    "HEADLESS-1",
                )?;
            }
        }
        if !plugin_paths.is_empty() {
            overview_checks(&mut h, &clients)?;
        }
        ensure!(
            h.capture(
                &clients,
                "direct-window",
                "window",
                "shm",
                "dev.lennette.overlay"
            )?
            .count([255, 0, 255])
                == 0,
            "Direct capture leaked private fixture"
        );
        h.checks
            .push(json!({"test":"direct protected window capture denied"}));
        h.lua("hl.monitor({output=\"WAYLAND-1\",mirror=\"HEADLESS-1\",scale=\"1\"})")?;
        pause(700);
        h.local_display("local visibility over fullscreen")?;
        h.dispatch("hl.dsp.focus({monitor=\"HEADLESS-1\"})")?;
        let public = h.fixture(&clients, "test.public", "00ff00", false)?;
        move_window(&h, 250, 180, "test.public")?;
        h.local_display("local visibility over competing pinned window")?;
        let frame = h.capture(&clients, "competing-pinned", "wlr", "shm", "HEADLESS-1")?;
        ensure!(
            frame.count([255, 0, 255]) == 0 && frame.count([0, 255, 0]) > 1000,
            "Public pinned export did not exclude overlay"
        );
        h.checks
            .push(json!({"test":"public pinned window is exported beneath private overlay"}));
        h.dispatch("hl.dsp.focus({monitor=\"HEADLESS-1\"})")?;
        move_window(&h, 250, 180, "dev.lennette.overlay")?;
        move_window(&h, 250, 180, "test.public")?;
        h.dispatch("hl.dsp.cursor.move({x=300,y=200})")?;
        h.call("wlrctl", &["pointer", "click"])?;
        h.wait("pointer focuses private overlay", 10, |h| {
            Ok(h.json(&["-j", "activewindow"])?["class"] == "dev.lennette.overlay")
        })?;
        h.checks
            .push(json!({"test":"pointer hit testing prioritizes private overlay"}));
        h.processes[public].stop();
        h.dispatch("hl.dsp.focus({window=\"class:test.background2\"})")?;
        h.dispatch("hl.dsp.focus({window=\"class:test.background$\"})")?;
        for (index, (x, y)) in [
            (0, 0),
            (200, 250),
            (800, 120),
            (930, 200),
            (1000, 70),
            (1250, 300),
        ]
        .into_iter()
        .enumerate()
        {
            move_window(&h, x, y, "dev.lennette.overlay")?;
            for output in ["HEADLESS-1", "HEADLESS-2"] {
                clean(
                    &mut h,
                    &clients,
                    &format!("move {index}, {output}"),
                    "wlr",
                    "shm",
                    output,
                )?;
            }
        }
        h.lua("hl.monitor({output=\"HEADLESS-2\",mode=\"800x600@60\",position=\"960x0\",scale=\"1.25\",transform=1})")?;
        pause(500);
        move_window(&h, 1000, 100, "dev.lennette.overlay")?;
        clean(
            &mut h,
            &clients,
            "rotation + fractional scale + DMA-BUF",
            "ext",
            "dma",
            "HEADLESS-2",
        )?;
        h.dispatch("hl.dsp.focus({monitor=\"HEADLESS-2\"})")?;
        let monitors = h.json(&["-j", "monitors"])?;
        let previous = monitors
            .as_array()
            .context("Invalid monitors")?
            .iter()
            .find(|m| m["name"] == "HEADLESS-2")
            .context("Missing HEADLESS-2")?["activeWorkspace"]["id"]
            .to_string();
        h.dispatch("hl.dsp.focus({workspace=\"8\"})")?;
        ensure!(
            h.window("dev.lennette.overlay")?["pinned"] == true,
            "Overlay lost pin on workspace change"
        );
        h.dispatch(&format!("hl.dsp.focus({{workspace={}}})", quote(previous)))?;
        h.dispatch("hl.dsp.window.set_prop({window=\"class:dev.lennette.overlay\",prop=\"private_overlay\",value=\"0\"})")?;
        let visible = h
            .capture(&clients, "positive-control", "wlr", "shm", "HEADLESS-2")?
            .count([255, 0, 255]);
        ensure!(
            visible > 1000,
            "Positive control failed: {visible} marker pixels"
        );
        h.dispatch("hl.dsp.window.set_prop({window=\"class:dev.lennette.overlay\",prop=\"private_overlay\",value=\"1\"})")?;
        clean(
            &mut h,
            &clients,
            "privacy restored",
            "wlr",
            "shm",
            "HEADLESS-2",
        )?;
        h.lua("hl.config({animations={enabled=true}})")?;
        h.processes[overlay].stop();
        for index in 0..8 {
            clean(
                &mut h,
                &clients,
                &format!("closing animation {index}"),
                "wlr",
                "shm",
                "HEADLESS-2",
            )?;
        }
        h.checks.push(json!({"test":"floating, pinned, workspace switching and positive control","visible_control_pixels":visible}));
        if let Some(binary) = &binary {
            h.dispatch("hl.dsp.focus({monitor=\"HEADLESS-1\"})")?;
            let shortcut_file = h.artifacts.join("user-config/hypr/lennette.lua");
            if let Some(source) = &shortcuts {
                use std::os::unix::fs::PermissionsExt;
                if shortcut_file.exists() {
                    fs::set_permissions(&shortcut_file, fs::Permissions::from_mode(0o644))?;
                }
                fs::write(&shortcut_file, fs::read(source)?)?;
                fs::set_permissions(&shortcut_file, fs::Permissions::from_mode(0o444))?;
                h.lua(&format!("dofile({})", path_quote(&shortcut_file)))?;
            }
            h.env
                .insert("LENNETTE_BINARY".into(), binary.as_os_str().into());
            let application =
                h.spawn(&root.join("packaging/hyprland/launch.sh"), &[], "lennette")?;
            let app_pid = h.processes[application].0.id();
            h.wait("real Lennette window", 30, |h| {
                Ok(h.clients()?
                    .iter()
                    .any(|w| w["class"] == "dev.lennette.overlay"))
            })?;
            move_window(&h, 250, 180, "dev.lennette.overlay")?;
            clean(
                &mut h,
                &clients,
                "real Lennette overlay excluded",
                "wlr",
                "shm",
                "HEADLESS-1",
            )?;
            h.dispatch("hl.dsp.window.set_prop({window=\"class:dev.lennette.overlay\",prop=\"private_overlay\",value=\"0\"})")?;
            let different = h
                .capture(
                    &clients,
                    "real-lennette-positive-control",
                    "wlr",
                    "shm",
                    "HEADLESS-1",
                )?
                .unexpected();
            ensure!(different > 1000, "Real app positive control failed");
            h.dispatch("hl.dsp.window.set_prop({window=\"class:dev.lennette.overlay\",prop=\"private_overlay\",value=\"1\"})")?;
            clean(
                &mut h,
                &clients,
                "real Lennette protection restored",
                "wlr",
                "shm",
                "HEADLESS-1",
            )?;
            h.checks.push(
                json!({"test":"real Lennette positive control","visible_control_pixels":different}),
            );
            ensure!(
                fs::read_to_string(h.artifacts.join("lennette.log"))?
                    .contains("tray disabled for private overlay"),
                "Private app tray was not suppressed"
            );
            h.checks
                .push(json!({"test":"real Lennette suppresses its tray in private mode"}));
            if let Some(source) = &shortcuts {
                ensure!(
                    protected_bindings(&h.json(&["-j", "binds"])?)
                        && fs::read(&shortcut_file)? == fs::read(source)?,
                    "Protected bindings missing or source overwritten"
                );
                h.checks
                    .push(json!({"test":"all six protected compositor bindings installed"}));
                keyboard(&mut h, &clients, app_pid)?;
            }
        }
        doty(&mut h, doty_path.as_deref(), &plugin_paths)
    })();
    let mut evidence = json!({"compositor":compositor,"drm_node":drm,"patch_sha256":crate::release::hash(&root.join("packaging/hyprland/private-overlay.patch"))?,"plugin_compat_sha256":crate::release::hash(&root.join("packaging/hyprland/plugin-compat.patch"))?,"plugins":loaded_plugins});
    if let Some(binary) = binary {
        evidence["lennette_sha256"] = json!(crate::release::hash(&binary)?);
    }
    h.record(&result, evidence)?;
    if result.is_ok() {
        println!(
            "PASS {} checks. Artifacts: {}",
            h.checks.len(),
            h.artifacts.display()
        );
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn binding_validation_requires_all_six_immutable_exec_bindings() {
        let mut bindings = json!(
            ["D", "backslash", "I", "M", "A", "S"]
                .map(|key| json!({"key":key,"modmask":5,"dispatcher":"HL.Dispatcher(exec_cmd)"}))
        );
        assert!(protected_bindings(&bindings));
        bindings[1]["keycode"] = json!(51);
        bindings[1]["key"] = json!("");
        assert!(protected_bindings(&bindings));
        bindings[3]["dispatcher"] = json!("wrong");
        assert!(!protected_bindings(&bindings));
    }
}
