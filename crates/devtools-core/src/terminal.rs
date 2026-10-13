//! Opening things at a repo: folders and solutions through the desktop, VS
//! Code with the configured profile, and a command (zcode, opencode, a shell)
//! inside the configured terminal emulator — each emulator has its own
//! working-directory and -e argument shapes.

use crate::exe;
use std::path::Path;

/// Opens a folder (or any path) with the desktop file manager.
pub fn open_folder(folder: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let command = "open";
    #[cfg(all(unix, not(target_os = "macos")))]
    let command = "xdg-open";
    #[cfg(windows)]
    let command = "explorer";
    std::process::Command::new(command)
        .arg(folder)
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("{command}: {err}"))
}

/// Opens a solution file: a configured IDE wins, else the shell association
/// (Windows) or the .NET-IDE fallback.
pub fn open_solution(solution: &str, ide_executable: &str) -> Result<(), String> {
    let configured = ide_executable.trim();
    if !configured.is_empty() {
        let Some(path) = exe::locate(configured) else {
            return Err(format!("'{configured}' not found on PATH or in the user install folders"));
        };
        return std::process::Command::new(&path)
            .arg(solution)
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("{}: {err}", path.display()));
    }
    #[cfg(windows)]
    {
        return std::process::Command::new("cmd")
            .args(["/C", "start", "", solution])
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("shell association: {err}"));
    }
    #[cfg(not(windows))]
    {
        let exe_name = "rider";
        let Some(path) = exe::locate(exe_name) else {
            return Err(format!("'{exe_name}' not found on PATH or in the user install folders"));
        };
        std::process::Command::new(&path)
            .arg(solution)
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("{}: {err}", path.display()))
    }
}

/// Opens a repo folder in VS Code, with the configured profile when set.
pub fn open_in_vscode(folder: &str, vscode_executable: &str, profile: &str) -> Result<(), String> {
    let Some(path) = exe::locate(if vscode_executable.trim().is_empty() { "code" } else { vscode_executable }) else {
        return Err("VS Code executable not found".into());
    };
    let mut command = std::process::Command::new(&path);
    if !profile.trim().is_empty() {
        command.args(["--profile", profile.trim()]);
    }
    command.arg(folder);
    command
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("{}: {err}", path.display()))
}

/// Opens a plain shell at the folder using the configured terminal (or the
/// first probed one).
pub fn open_terminal(folder: &str, terminal_executable: &str) -> Result<(), String> {
    let configured = terminal_executable.trim();
    if !configured.is_empty() {
        let base = configured
            .rsplit('/')
            .next()
            .unwrap_or(configured)
            .trim_end_matches(".exe");
        let candidates = [configured.to_string(), base.to_string()];
        for name in candidates {
            if let Some(path) = exe::locate(&name) {
                let mut command = std::process::Command::new(&path);
                apply_open_dir(&mut command, base, folder);
                if Path::new(folder).exists() {
                    command.current_dir(folder);
                }
                return command.spawn().map(|_| ()).map_err(|err| format!("{}: {err}", path.display()));
            }
        }
        // configured-but-missing: fall through to probing only when unresolved
    }
    for name in probed_terminals() {
        if let Some(path) = exe::locate(name) {
            let mut command = std::process::Command::new(&path);
            apply_open_dir(&mut command, name, folder);
            if Path::new(folder).exists() {
                command.current_dir(folder);
            }
            return command.spawn().map(|_| ()).map_err(|err| format!("{}: {err}", path.display()));
        }
    }
    Err("No terminal emulator found (tried the configured one plus the common ones)".into())
}

fn probed_terminals() -> &'static [&'static str] {
    &["alacritty", "ghostty", "foot", "kitty", "wezterm", "gnome-terminal", "konsole", "xterm"]
}

/// Runs a command (zcode, opencode, …) inside a terminal at the folder.
pub fn launch_in_terminal(folder: &str, command: &str, terminal_executable: &str) -> Result<(), String> {
    launch_argv_in_terminal(folder, &[command.to_string()], terminal_executable)
}

/// Runs a full argv (e.g. `opencode --model <id>`) inside a terminal at the
/// folder — the quick-launch path, where the command carries arguments.
pub fn launch_argv_in_terminal(folder: &str, argv: &[String], terminal_executable: &str) -> Result<(), String> {
    if argv.is_empty() {
        return Err("nothing to launch".into());
    }
    type ArgsFn = fn(&str, &[String]) -> Vec<String>;
    let table: &[(&str, ArgsFn)] = &[
        ("alacritty", |dir, cmd| {
            vec!["--working-directory".into(), dir.into(), "-e".into()]
                .into_iter()
                .chain(cmd.iter().cloned())
                .collect()
        }),
        ("ghostty", |dir, cmd| {
            vec![format!("--working-directory={dir}"), "-e".into()]
                .into_iter()
                .chain(cmd.iter().cloned())
                .collect()
        }),
        ("foot", |dir, cmd| {
            vec![format!("--working-directory={dir}")]
                .into_iter()
                .chain(cmd.iter().cloned())
                .collect()
        }),
        ("kitty", |dir, cmd| {
            vec!["--directory".into(), dir.into()]
                .into_iter()
                .chain(cmd.iter().cloned())
                .collect()
        }),
        ("wezterm", |dir, cmd| {
            vec!["start".into(), "--cwd".into(), dir.into(), "--".into()]
                .into_iter()
                .chain(cmd.iter().cloned())
                .collect()
        }),
        ("gnome-terminal", |dir, cmd| {
            vec![format!("--working-directory={dir}"), "--".into()]
                .into_iter()
                .chain(cmd.iter().cloned())
                .collect()
        }),
        ("konsole", |dir, cmd| {
            vec!["--workdir".into(), dir.into(), "-e".into()]
                .into_iter()
                .chain(cmd.iter().cloned())
                .collect()
        }),
        ("xterm", |_dir, cmd| {
            vec!["-e".into()].into_iter().chain(cmd.iter().cloned()).collect()
        }),
    ];
    // the configured terminal wins when it names a known shape
    let configured_base = terminal_executable
        .trim()
        .rsplit('/')
        .next()
        .unwrap_or("")
        .trim_end_matches(".exe")
        .to_string();
    let mut ordered: Vec<(&str, ArgsFn)> = Vec::new();
    for entry in table {
        if !configured_base.is_empty() && *entry.0 == configured_base {
            ordered.insert(0, *entry);
        } else {
            ordered.push(*entry);
        }
    }
    for (name, args) in ordered {
        if let Some(path) = exe::locate(name) {
            let mut command_builder = std::process::Command::new(&path);
            command_builder.args(args(folder, argv));
            if Path::new(folder).exists() {
                command_builder.current_dir(folder);
            }
            match command_builder.spawn() {
                Ok(_) => {
                    tracing::info!("launched {name} running {:?} in {folder}", argv);
                    return Ok(());
                }
                Err(err) => return Err(err.to_string()),
            }
        }
    }
    Err("No terminal emulator found on PATH.".into())
}

fn apply_open_dir(command: &mut std::process::Command, terminal: &str, folder: &str) {
    match terminal {
        "alacritty" => {
            command.args(["--working-directory", folder]);
        }
        "ghostty" | "foot" => {
            command.arg(format!("--working-directory={folder}"));
        }
        "kitty" => {
            command.args(["--directory", folder]);
        }
        "wezterm" => {
            command.args(["start", "--cwd", folder]);
        }
        "gnome-terminal" => {
            command.arg(format!("--working-directory={folder}"));
        }
        "konsole" => {
            command.args(["--workdir", folder]);
        }
        _ => {}
    }
}
