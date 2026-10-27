//! Avoid waking a discrete NVIDIA GPU for the assistant's small UI.

#[cfg(target_os = "linux")]
const GPU_OVERRIDES: &[&str] = &[
    "ZED_DEVICE_ID",
    "VK_DRIVER_FILES",
    "VK_ICD_FILENAMES",
    "VK_LOADER_DRIVERS_SELECT",
    "VK_LOADER_DRIVERS_DISABLE",
    "DRI_PRIME",
    "__NV_PRIME_RENDER_OFFLOAD",
    "__VK_LAYER_NV_optimus",
];

/// Select Intel's driver on an Intel-boot/NVIDIA hybrid laptop, unless the
/// user or launcher already selected a GPU. Call before starting the UI runtime.
pub fn preferred_vulkan_driver() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let overridden = GPU_OVERRIDES
            .iter()
            .any(|key| std::env::var_os(key).is_some());
        let directories = [
            std::path::Path::new("/run/opengl-driver/share/vulkan/icd.d"),
            std::path::Path::new("/usr/share/vulkan/icd.d"),
            std::path::Path::new("/etc/vulkan/icd.d"),
        ];
        discover_driver(
            std::path::Path::new("/sys/class/drm"),
            &directories,
            overridden,
        )
    }
    #[cfg(not(target_os = "linux"))]
    None
}

#[cfg(target_os = "linux")]
fn discover_driver(
    drm: &std::path::Path,
    directories: &[&std::path::Path],
    overridden: bool,
) -> Option<String> {
    if overridden {
        return None;
    }
    let mut intel_boot = false;
    let mut nvidia = false;
    for card in std::fs::read_dir(drm).ok()?.flatten() {
        let device = card.path().join("device");
        let vendor = std::fs::read_to_string(device.join("vendor")).unwrap_or_default();
        match vendor.trim() {
            "0x8086" => {
                intel_boot |= std::fs::read_to_string(device.join("boot_vga"))
                    .is_ok_and(|boot| boot.trim() == "1");
            }
            "0x10de" => nvidia = true,
            _ => {}
        }
    }
    if !intel_boot || !nvidia {
        return None;
    }
    let name = format!("intel_icd.{}.json", std::env::consts::ARCH);
    directories
        .iter()
        .any(|dir| dir.join(&name).is_file())
        .then_some(name)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Hardware(PathBuf);
    impl Hardware {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("nixarchy-meetingai-gpu-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(root.join("drm")).unwrap();
            std::fs::create_dir_all(root.join("icd")).unwrap();
            Self(root)
        }
        fn card(&self, name: &str, vendor: &str, boot: bool) {
            let device = self.0.join("drm").join(name).join("device");
            std::fs::create_dir_all(&device).unwrap();
            std::fs::write(device.join("vendor"), vendor).unwrap();
            std::fs::write(device.join("boot_vga"), if boot { "1" } else { "0" }).unwrap();
        }
        fn driver(&self) {
            std::fs::write(
                self.0
                    .join("icd")
                    .join(format!("intel_icd.{}.json", std::env::consts::ARCH)),
                "{}",
            )
            .unwrap();
        }
        fn selected(&self, overridden: bool) -> Option<String> {
            discover_driver(&self.0.join("drm"), &[&self.0.join("icd")], overridden)
        }
    }
    impl Drop for Hardware {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn hybrid_laptop_prefers_intel_without_overriding_explicit_selection() {
        let hw = Hardware::new();
        hw.card("card0", "0x8086\n", true);
        hw.card("card1", "0x10de\n", false);
        hw.driver();
        assert!(hw.selected(false).is_some());
        assert!(hw.selected(true).is_none());
    }

    #[test]
    fn discrete_boot_gpu_keeps_the_loader_default() {
        let hw = Hardware::new();
        hw.card("card0", "0x8086", false);
        hw.card("card1", "0x10de", true);
        hw.driver();
        assert!(hw.selected(false).is_none());
    }

    #[test]
    fn single_gpu_keeps_the_loader_default() {
        let hw = Hardware::new();
        hw.card("card0", "0x8086", true);
        hw.driver();
        assert!(hw.selected(false).is_none());
    }

    #[test]
    fn missing_intel_manifest_keeps_the_loader_default() {
        let hw = Hardware::new();
        hw.card("card0", "0x8086", true);
        hw.card("card1", "0x10de", false);
        assert!(hw.selected(false).is_none());
    }
}
