// The binary reuses the library's modules so the CLI and the app agree.
use lennette::cli;

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
    lennette::run();
}
