//! `nixarchy-meetingai` runs the daemon; `nixarchy-meetingai ctl <cmd>` talks to it.
use nixarchy_meetingai::cli;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some((output, code)) = cli::run_cli(&args) {
        if !output.is_empty() {
            println!("{output}");
        }
        return ExitCode::from(u8::try_from(code).unwrap_or(1));
    }
    // Held until the process exits; the kernel releases it on a crash too.
    let Some(_lock) = cli::InstanceLock::acquire() else {
        eprintln!("nixarchy-meetingai: another instance is already running");
        return ExitCode::FAILURE;
    };
    match nixarchy_meetingai::daemon::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("nixarchy-meetingai: {error}");
            ExitCode::FAILURE
        }
    }
}
