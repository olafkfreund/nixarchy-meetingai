//! `nixarchy-meetingai` runs the daemon; `nixarchy-meetingai ctl <cmd>` talks to it.
use nixarchy_meetingai::cli;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some((output, code)) = cli::run_cli(&args) {
        if !output.is_empty() {
            println!("{output}");
        }
        std::process::exit(code);
    }
    // Held until the process exits; the kernel releases it on a crash too.
    let Some(_instance) = cli::InstanceLock::acquire() else {
        eprintln!("nixarchy-meetingai: another instance is already running");
        std::process::exit(1);
    };
    if let Err(error) = nixarchy_meetingai::daemon::run() {
        eprintln!("nixarchy-meetingai: {error}");
        std::process::exit(1);
    }
}
