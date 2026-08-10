use std::process::Command;

fn run(binary: &str, args: &[&str]) -> String {
    let output = Command::new(binary)
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("failed to start {binary}: {error}"));
    assert!(
        output.status.success(),
        "{binary} {:?} failed:\nstdout={}\nstderr={}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn digicode_reports_the_governed_build_version() {
    let version = run(env!("CARGO_BIN_EXE_digicode"), &["--version"]);
    assert!(version.contains(env!("CARGO_PKG_VERSION")), "{version}");
}

#[test]
fn digicode_and_jcode_are_the_same_capable_build() {
    let digicode = std::fs::read(env!("CARGO_BIN_EXE_digicode")).expect("digicode binary");
    let jcode = std::fs::read(env!("CARGO_BIN_EXE_jcode")).expect("jcode binary");
    assert_eq!(
        digicode, jcode,
        "compatibility launcher must use the same build"
    );

    let help = run(env!("CARGO_BIN_EXE_digicode"), &["--help"]);
    for capability in ["run", "server", "version"] {
        assert!(
            help.contains(capability),
            "digicode help omitted {capability}: {help}"
        );
    }
}
