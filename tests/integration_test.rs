// Integration tests for Tether

#[cfg(test)]
mod tests {
    use std::process::Command;

    #[test]
    fn test_binary_exists() {
        let output = Command::new(env!("CARGO_BIN_EXE_tether"))
            .arg("--help")
            .output()
            .expect("Failed to execute tether binary");

        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("serve"));
        assert!(stdout.contains("Remote Terminal Controller"));
    }

    #[test]
    fn test_serve_help() {
        let output = Command::new(env!("CARGO_BIN_EXE_tether"))
            .arg("serve")
            .arg("--help")
            .output()
            .expect("Failed to execute tether serve --help");

        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("-p, --password"));
        assert!(stdout.contains("-P, --port"));
        assert!(stdout.contains("--allow-lan"));
    }
}
