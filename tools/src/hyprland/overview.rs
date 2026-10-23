use super::*;

const CONFIG: &str = r#"
hl.monitor({output="WAYLAND-1",mode="960x640@60",scale="1"})
hl.config({animations={enabled=false},misc={disable_hyprland_logo=true,disable_splash_rendering=true},ecosystem={no_update_news=true,no_donation_nag=true}})
hl.window_rule({match={class="test.overview.*"},float=true})
"#;
const BUTTONS: &str = r#"
hl.config({plugin={hyprbars={bar_height=28,bar_text_size=12}}})
hl.plugin.hyprbars.add_button({bg_color="rgb(ff5555)",fg_color="rgb(ffffff)",size=18,icon="X",action="true"})
hl.plugin.hyprbars.add_button({bg_color="rgb(55ff55)",fg_color="rgb(ffffff)",size=14,icon="+",action="true"})
hl.plugin.hyprbars.add_button({bg_color="rgb(5555ff)",fg_color="rgb(ffffff)",size=12,icon="-",action="true"})
"#;
pub(super) const OVERVIEWS: [(&str, &str, &str); 2] = [
    (
        "scrolloverview",
        "hl.dispatch(hl.plugin.scrolloverview.overview(\"open WAYLAND-1\"))",
        "hl.dispatch(hl.plugin.scrolloverview.overview(\"close\"))",
    ),
    (
        "hymission",
        "hl.plugin.hymission.open()",
        "hl.plugin.hymission.close()",
    ),
];

fn cycle(h: &mut Harness, label: &str, cycles: usize) -> Result<()> {
    for (name, opening, closing) in OVERVIEWS {
        for _ in 0..cycles {
            h.lua(opening)?;
            pause(200);
            ensure!(!h.clients()?.is_empty(), "No clients after opening {name}");
            h.lua(closing)?;
            pause(150);
        }
        h.checks
            .push(json!({"test":name,"phase":label,"cycles":cycles}));
        println!("PASS {name} {label}");
    }
    Ok(())
}
fn hidden(h: &Harness, pid: u32) -> Result<bool> {
    Ok(h.app_window(pid)?["workspace"]["name"] == "special:lennette-hidden")
}

pub fn run(root: &Path, args: &[String]) -> Result<()> {
    if args.iter().any(|s| s == "--help") {
        println!(
            "cargo xtask hyprland-overview --compositor PATH --plugins JSON --lennette PATH --client-dir PATH --artifacts DIR [--cycles 10] [--parent-socket PATH]"
        );
        return Ok(());
    }
    let mut args = Arguments::parse(args, &[])?;
    let compositor = absolute(&args.required("--compositor")?)?;
    let plugin_paths = plugins(Some(args.required("--plugins")?))?;
    let binary = absolute(&args.required("--lennette")?)?;
    let clients = absolute(&args.required("--client-dir")?)?;
    let artifacts = args.required("--artifacts")?;
    let cycles = args
        .take("--cycles")
        .unwrap_or_else(|| "10".into())
        .parse::<usize>()?;
    ensure!(
        (1..=1000).contains(&cycles),
        "Cycles must be between 1 and 1000"
    );
    let parent = args.take("--parent-socket");
    args.finish()?;
    let mut h = Harness::new(&artifacts, parent, true)?;
    let mut config = CONFIG.to_string();
    for plugin in &plugin_paths {
        config.push_str(&format!("hl.plugin.load({})\n", quote(plugin)));
    }
    config.push_str(BUTTONS);
    let result = (|| -> Result<()> {
        h.start(&compositor, &config)?;
        let loaded = h.json(&["-j", "plugin", "list"])?;
        let mut actual: Vec<_> = loaded
            .as_array()
            .context("Invalid plugin list")?
            .iter()
            .filter_map(|p| p["path"].as_str().map(String::from))
            .collect();
        actual.sort();
        let mut expected = plugin_paths.clone();
        expected.sort();
        ensure!(
            actual == expected,
            "Loaded plugins differ from requested builds"
        );
        for index in 0..3 {
            h.fixture(&clients, &format!("test.overview{index}"), "206080", false)?;
        }
        cycle(&mut h, "without Lennette", cycles)?;
        for mode in ["normal", "private"] {
            if mode == "private" {
                h.env.insert("LENNETTE_PRIVATE_OVERLAY".into(), "1".into());
                h.lua(&format!(
                    "dofile({})",
                    path_quote(&root.join("packaging/hyprland/lennette-private.lua"))
                ))?;
            }
            let start = Instant::now();
            let index = h.spawn(&binary, &[], &format!("lennette-{mode}"))?;
            let pid = h.processes[index].0.id();
            h.wait("app window", 30, |h| {
                Ok(h.clients()?.iter().any(|w| w["pid"] == pid))
            })?;
            let elapsed = start.elapsed().as_secs_f64();
            println!("MAPPED {mode} {elapsed:.3} seconds");
            cycle(&mut h, &format!("{mode} Lennette"), cycles)?;
            let close = |h: &Harness| -> Result<()> {
                let window = h.app_window(pid)?;
                h.dispatch(&format!(
                    "hl.dsp.window.close({{window={}}})",
                    quote(format!(
                        "address:{}",
                        window["address"].as_str().context("Missing address")?
                    ))
                ))
            };
            close(&h)?;
            if mode == "normal" {
                h.wait("close hides into tray", 15, |h| hidden(h, pid))?;
                ensure!(
                    h.processes[index].0.try_wait()?.is_none(),
                    "Normal app unexpectedly exited"
                );
                ensure!(
                    fs::read_to_string(h.artifacts.join("lennette-normal.log"))?
                        .contains("tray icon available"),
                    "Normal tray was unavailable"
                );
                // A second launch and focus shortcut must not bypass hidden-state filtering.
                h.call(&binary, &[])?;
                h.call(&binary, &["--focus-input"])?;
                pause(300);
                ensure!(
                    hidden(&h, pid)?,
                    "A non-toggle command restored a hidden app"
                );
                ensure!(
                    h.clients()?.iter().filter(|w| w["pid"] == pid).count() == 1,
                    "Second launch created another window"
                );
                h.call(&binary, &["--toggle-window"])?;
                h.wait("visibility shortcut restores existing app", 15, |h| {
                    Ok(!hidden(h, pid)?)
                })?;
                close(&h)?;
                h.wait("second close hides", 15, |h| hidden(h, pid))?;
                h.call(&binary, &["--toggle-window"])?;
                h.wait("second visibility restore", 15, |h| Ok(!hidden(h, pid)?))?;
                h.processes[index].stop();
            } else {
                ensure!(
                    h.processes[index].wait(Duration::from_secs(5))?.success(),
                    "Private close did not exit cleanly"
                );
                ensure!(
                    fs::read_to_string(h.artifacts.join("lennette-private.log"))?
                        .contains("tray disabled for private overlay"),
                    "Private app created a tray"
                );
            }
            h.checks
                .push(json!({"test":format!("{mode} lifecycle"),"startup_seconds":elapsed}));
        }
        Ok(())
    })();
    h.record(&result, json!({"plugins":plugin_paths}))?;
    result
}
