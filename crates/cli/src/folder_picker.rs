//! Native folder selection on the computer running the panel.

use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{Context, Result, bail};

use crate::procs::command;

static OPEN: Mutex<()> = Mutex::new(());

pub(crate) fn pick() -> Result<Option<PathBuf>> {
    let _open = OPEN.try_lock().map_err(|_| anyhow::anyhow!("a folder picker is already open"))?;
    native_picker()
}

#[cfg(target_os = "windows")]
fn native_picker() -> Result<Option<PathBuf>> {
    // The app and the local browser share this dialog. PowerShell launches it
    // without a console window, and writes the selected path as UTF-8.
    let script = r#"
        $ErrorActionPreference = 'Stop'
        Add-Type -AssemblyName System.Windows.Forms
        [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
        $dialog = New-Object System.Windows.Forms.FolderBrowserDialog
        $dialog.ShowNewFolderButton = $false
        try {
            if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
                [Console]::Write($dialog.SelectedPath)
            }
        } finally { $dialog.Dispose() }
    "#;
    let output =
        command("powershell.exe").args(["-NoProfile", "-NonInteractive", "-STA", "-Command", script]).output()?;
    selected(output, false)
}

#[cfg(target_os = "macos")]
fn native_picker() -> Result<Option<PathBuf>> {
    let output = command("osascript")
        .args([
            "-e",
            "try",
            "-e",
            "return POSIX path of (choose folder)",
            "-e",
            "on error message number code",
            "-e",
            "if code is -128 then return \"\"",
            "-e",
            "error message number code",
            "-e",
            "end try",
        ])
        .output()?;
    selected(output, true)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn native_picker() -> Result<Option<PathBuf>> {
    for (program, args) in
        [("zenity", &["--file-selection", "--directory"][..]), ("kdialog", &["--getexistingdirectory", "."][..])]
    {
        match command(program).args(args).output() {
            Ok(output) if output.status.code() == Some(1) && output.stderr.is_empty() => return Ok(None),
            Ok(output) => return selected(output, true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        }
    }
    bail!("install zenity or kdialog to choose a folder")
}

#[cfg(not(any(unix, windows)))]
fn native_picker() -> Result<Option<PathBuf>> {
    bail!("the folder picker is unavailable on this system")
}

#[cfg(any(unix, windows))]
fn selected(output: std::process::Output, newline: bool) -> Result<Option<PathBuf>> {
    if !output.status.success() {
        bail!("could not open the folder picker: {}", String::from_utf8_lossy(&output.stderr).trim());
    }
    let path = String::from_utf8(output.stdout).context("the folder path must be UTF-8")?;
    let path = if newline { path.strip_suffix('\n').unwrap_or(&path) } else { &path };
    Ok((!path.is_empty()).then(|| PathBuf::from(path)))
}
