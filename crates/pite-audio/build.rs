// SPDX-License-Identifier: MIT OR Apache-2.0

fn main() {
    #[cfg(target_os = "linux")]
    {
        let has_alsa = std::process::Command::new("pkg-config")
            .args(["--exists", "alsa"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if !has_alsa {
            let lib_exists = [
                "/usr/lib",
                "/usr/local/lib",
                "/usr/lib/x86_64-linux-gnu",
                "/usr/lib/aarch64-linux-gnu",
            ]
            .iter()
            .any(|p| std::path::Path::new(p).join("libasound.so").exists());

            if !lib_exists {
                println!("cargo:warning=");
                println!("cargo:warning=ALSA development headers ('alsa' via pkg-config) were not detected.");
                println!("cargo:warning=On Ubuntu/Debian: sudo apt-get install libasound2-dev pkg-config");
                println!("cargo:warning=On Fedora/RHEL:   sudo dnf install alsa-lib-devel pkgconf-pkg-config");
                println!("cargo:warning=On Arch Linux:     sudo pacman -S alsa-lib pkgconf");
                println!("cargo:warning=");
            }
        }
    }
}
