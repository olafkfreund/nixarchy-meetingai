/// Run `pactl`, killed after two seconds so a wedged server cannot hang callers.
pub(crate) fn pactl(args: &[&str]) -> Result<String, String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut child = Command::new("pactl")
        // TODO step3: pass `args` to the command
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        // TODO step3: map a spawn failure to Err(String) naming pactl and the OS error
    let mut stdout = child.stdout.take().ok_or("pactl gave no output")?;
    let (tx, rx) = std::sync::mpsc::channel();
    // Read on a thread so a full pipe cannot block the child before the deadline.
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        let _ = tx.send(text);
    });
    match rx.recv_timeout(std::time::Duration::from_secs(2)) {
        Ok(text) => match child.wait() {
            Ok(status) if status.success() => Ok(text),
            _ => Err("pactl failed".into()),
        },
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            Err("pactl timed out".into())
        }
        // TODO step3: close the Err arm, the match and the function
        } } }
