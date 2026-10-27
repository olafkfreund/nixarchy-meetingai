// The binary reuses the library's modules so the CLI and the app agree.
use nixarchy_meetingai::cli;

fn main() {
    // Shortcut flags run one-shot and exit; anything else launches the app.
    if let Some(output) = cli::handle_args() {
        if !output.is_empty() {
            println!("{output}");
        }
        return;
    }
    // If an instance is already running, activate its window instead of colliding.
    if cli::activate_existing_instance() {
        return;
    }
    // Claim the instance slot before any expensive startup work. The socket check
    // above can miss a process that is still starting, so this atomic lock is what
    // actually prevents a second copy. Held until the process exits.
    let Some(_instance) = cli::InstanceLock::acquire() else {
        // Another launch won the race. It may still be starting, before its socket
        // exists, so retry briefly: a second keypress should surface the window,
        // never vanish. If it never answers, exit rather than start a second copy.
        if !cli::wait_for_running_instance(std::time::Duration::from_secs(15)) {
            println!(
                "nixarchy-meetingai: another instance is running but did not accept the request"
            );
        }
        return;
    };
    if let Some(driver) = nixarchy_meetingai::platform::system::gpu::preferred_vulkan_driver() {
        // No worker threads exist yet; the Vulkan loader reads this during UI init.
        unsafe { std::env::set_var("VK_LOADER_DRIVERS_SELECT", driver) };
    }
    nixarchy_meetingai::run();
}
