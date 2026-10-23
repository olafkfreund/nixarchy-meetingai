//! Local capability probe. Never uploads or retains audio or screen pixels.
use std::time::Duration;

fn main() {
    let devices = lennette::audio::devices::enumerate();
    println!(
        "Audio enumeration: {} inputs, {} outputs",
        devices.inputs.len(),
        devices.outputs.len()
    );
    if let Some(error) = devices.error {
        println!("Audio enumeration notice: {error}");
    }
    match lennette::audio::Microphone::start(None) {
        Ok(mic) => {
            std::thread::sleep(Duration::from_secs(1));
            println!(
                "Microphone opened: {}; received {} local samples; running={}",
                mic.device_name(),
                mic.take(32_000).len(),
                mic.is_running()
            );
            mic.stop();
        }
        Err(error) => println!("Microphone access failed: {error}"),
    }
    match lennette::audio::system_audio::SystemAudio::start(None) {
        Ok(mut audio) => {
            std::thread::sleep(Duration::from_secs(1));
            let samples = audio.take(32_000).len();
            if let Some(error) = audio.failure() {
                println!("System loopback error: {error}");
            }
            println!(
                "System loopback opened: {}; received {} local samples; running={}",
                audio.source(),
                samples,
                audio.is_running()
            );
            audio.stop();
        }
        Err(error) => println!("System loopback access failed: {error}"),
    }
    match lennette::platform::screen::capture_monitor(None) {
        Ok(frame) => println!(
            "Screen capture: {} x {} pixels, discarded",
            frame.width(),
            frame.height()
        ),
        Err(error) => println!("Screen capture failed: {error}"),
    }
    println!("No audio or screen pixels were saved or sent to a provider.");
}
