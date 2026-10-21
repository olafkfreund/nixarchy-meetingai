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
    lennette::run();
}
