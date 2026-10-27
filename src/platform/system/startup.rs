//! Per-user startup registration. No elevated installation is needed.

pub fn set_enabled(enabled: bool, private: bool) -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let mode = if private { "--privacy" } else { "--normal" };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
        let mut command = std::process::Command::new("reg.exe");
        command.creation_flags(0x08000000);
        if enabled {
            command
                .args(["add", key, "/v", "nixarchy-meetingai", "/t", "REG_SZ", "/d"])
                .arg(format!("\"{}\" {mode}", executable.display()))
                .arg("/f");
        } else {
            // Deleting an already absent registration is a successful no-op.
            let exists = std::process::Command::new("reg.exe")
                .creation_flags(0x08000000)
                .args(["query", key, "/v", "nixarchy-meetingai"])
                .output()
                .map_err(|e| e.to_string())?;
            if !exists.status.success() {
                return Ok(());
            }
            command.args(["delete", key, "/v", "nixarchy-meetingai", "/f"]);
        }
        let output = command.output().map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(format!(
                "Windows could not update startup: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
    }
    #[cfg(target_os = "linux")]
    {
        let config = directories::BaseDirs::new().ok_or("No user configuration folder")?;
        let directory = config.config_dir().join("autostart");
        let path = directory.join("dev.nixarchy.meetingai.desktop");
        if enabled {
            std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
            let executable = executable
                .to_string_lossy()
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('`', "\\`")
                .replace('$', "\\$")
                .replace('%', "%%");
            std::fs::write(path, format!("[Desktop Entry]\nType=Application\nName=nixarchy-meetingai\nExec=\"{executable}\" {mode}\nIcon=dev.nixarchy.meetingai\nTerminal=false\n")).map_err(|e| e.to_string())?;
        } else if path.exists() {
            std::fs::remove_file(path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
