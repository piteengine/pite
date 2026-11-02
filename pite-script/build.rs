// SPDX-License-Identifier: MIT OR Apache-2.0

fn main() {
    println!("cargo:rerun-if-env-changed=PYO3_PYTHON");
    println!("cargo:rerun-if-env-changed=LD_LIBRARY_PATH");

    let py = std::env::var("PYO3_PYTHON").unwrap_or_else(|_| "python3.12".to_string());
    let status = std::process::Command::new(&py)
        .arg("-c")
        .arg("import sys; assert sys.version_info >= (3, 12)")
        .status();

    if status.is_err() || !status.as_ref().map(|s| s.success()).unwrap_or(false) {
        let has_py = ["python3", "python"].iter().any(|cmd| {
            std::process::Command::new(cmd)
                .arg("-c")
                .arg("import sys; assert sys.version_info >= (3, 12)")
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        });

        if !has_py {
            println!("cargo:warning=");
            println!("cargo:warning=Python 3.12 with development headers was not detected.");
            println!(
                "cargo:warning=Pite requires Python 3.12 to build its embedded scripting engine."
            );
            println!("cargo:warning=On Ubuntu/Debian: sudo apt-get install python3.12-dev");
            println!("cargo:warning=On macOS:         brew install python@3.12");
            println!("cargo:warning=On Windows:       install Python 3.12 with dev headers from python.org");
            println!("cargo:warning=");
        }
    }
}
