//! Run a command inside an isolated headless Sway compositor.
//!
//! This owns only the compositor it starts. Runtime files are isolated under a
//! private temporary XDG runtime directory and removed after the child command
//! and compositor have exited.

use anyhow::{bail, Context, Result};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const START_TIMEOUT: Duration = Duration::from_secs(10);
const STOP_TIMEOUT: Duration = Duration::from_secs(5);

struct OwnedCompositor {
    child: Child,
    runtime_dir: PathBuf,
    sway_socket: PathBuf,
}

impl OwnedCompositor {
    fn start() -> Result<Self> {
        require_command("sway")?;
        let runtime_dir = private_runtime_dir()?;
        let config_path = runtime_dir.join("sway.conf");
        fs::write(
            &config_path,
            "output * mode 1280x720@60Hz\ninput * xkb_layout us\nfocus_follows_mouse no\n",
        )
        .context("failed to write headless Sway config")?;

        let mut child = Command::new("sway")
            .args(["--unsupported-gpu", "--config"])
            .arg(&config_path)
            .env("WLR_BACKENDS", "headless")
            .env("WLR_LIBINPUT_NO_DEVICES", "1")
            .env("XDG_RUNTIME_DIR", &runtime_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .context("failed to start headless Sway")?;

        let sway_socket = match wait_for_sway_socket(&runtime_dir) {
            Ok(socket) => socket,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = fs::remove_dir_all(&runtime_dir);
                return Err(error);
            }
        };
        Ok(Self {
            child,
            runtime_dir,
            sway_socket,
        })
    }

    fn command(&self, program: &str, args: &[String]) -> Command {
        let mut command = Command::new(program);
        command
            .args(args)
            .env_remove("NIRI_SOCKET")
            .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
            .env_remove("KDE_FULL_SESSION")
            .env_remove("GNOME_SHELL_SESSION_MODE")
            .env("XDG_RUNTIME_DIR", &self.runtime_dir)
            .env("SWAYSOCK", &self.sway_socket)
            .env("I3SOCK", &self.sway_socket)
            .env("XDG_CURRENT_DESKTOP", "sway")
            .env("XDG_SESSION_TYPE", "wayland")
            .env(
                "WAYLAND_DISPLAY",
                wayland_display(&self.runtime_dir).unwrap_or_else(|| "wayland-1".into()),
            )
            .env("JCODE_COMPUTER_USE_SCREENSHOT_BACKEND", "grim");
        command
    }

    fn stop(&mut self) {
        let _ = Command::new("swaymsg")
            .args(["-s", self.sway_socket.to_string_lossy().as_ref(), "exit"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let deadline = Instant::now() + STOP_TIMEOUT;
        while Instant::now() < deadline {
            match self.child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => thread::sleep(Duration::from_millis(50)),
                Err(_) => break,
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for OwnedCompositor {
    fn drop(&mut self) {
        self.stop();
        let _ = fs::remove_dir_all(&self.runtime_dir);
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("jcode-computer-use-headless: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<u8> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--") {
        args.remove(0);
    }
    let Some(program) = args.first().cloned() else {
        bail!("usage: jcode-computer-use-headless -- <command> [args...]");
    };
    let command_args = &args[1..];
    let compositor = OwnedCompositor::start()?;
    let status = compositor
        .command(&program, command_args)
        .status()
        .with_context(|| format!("failed to run {program:?} inside headless Sway"))?;
    Ok(status.code().unwrap_or(1).clamp(0, 255) as u8)
}

fn require_command(name: &str) -> Result<()> {
    let status = Command::new(name)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match status {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            bail!("required command {name:?} was not found on PATH")
        }
        Err(error) => Err(error).with_context(|| format!("failed to probe {name:?}")),
    }
}

fn private_runtime_dir() -> Result<PathBuf> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("jcode-headless-{}-{stamp}", std::process::id()));
    fs::create_dir(&path).context("failed to create private headless runtime directory")?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
        .context("failed to secure private headless runtime directory")?;
    Ok(path)
}

fn wait_for_sway_socket(runtime_dir: &Path) -> Result<PathBuf> {
    let deadline = Instant::now() + START_TIMEOUT;
    while Instant::now() < deadline {
        if let Some(path) = find_socket(runtime_dir, "sway-ipc.") {
            return Ok(path);
        }
        thread::sleep(Duration::from_millis(50));
    }
    bail!("headless Sway did not publish its IPC socket within {START_TIMEOUT:?}")
}

fn wayland_display(runtime_dir: &Path) -> Option<String> {
    find_socket(runtime_dir, "wayland-").and_then(|path| {
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
    })
}

fn find_socket(runtime_dir: &Path, prefix: &str) -> Option<PathBuf> {
    fs::read_dir(runtime_dir).ok()?.flatten().find_map(|entry| {
        let name = entry.file_name();
        name.to_string_lossy()
            .starts_with(prefix)
            .then(|| entry.path())
    })
}
