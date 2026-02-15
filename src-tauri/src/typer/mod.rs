use arboard::Clipboard;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::process::Command;
use std::thread;
use std::time::Duration;

/// Detect if we're running on Wayland.
#[cfg(target_os = "linux")]
fn is_wayland() -> bool {
    // $WAYLAND_DISPLAY is set by all Wayland compositors when a session is active
    std::env::var("WAYLAND_DISPLAY").is_ok()
        || std::env::var("XDG_SESSION_TYPE")
            .map(|v| v == "wayland")
            .unwrap_or(false)
}

/// Check whether an external command exists on $PATH.
#[cfg(target_os = "linux")]
fn command_exists(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Clipboard paste via xclip + xdotool on X11.
///
/// Uses xclip to set the clipboard, then xdotool to simulate a single
/// Ctrl+V key combo. This is much faster than `xdotool type` which
/// simulates individual keystrokes and introduces noticeable lag.
///
/// Flow: save clipboard -> set text -> Ctrl+V -> wait -> restore clipboard.
#[cfg(target_os = "linux")]
fn type_via_xclip_paste(text: &str) -> Result<(), String> {
    use std::io::Write;
    use std::process::Stdio;

    // 1. Save current clipboard contents (ok to fail if clipboard is empty/non-text)
    let previous = Command::new("xclip")
        .args(["-selection", "clipboard", "-o"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok()
            } else {
                None
            }
        });

    // 2. Set clipboard to our text via stdin pipe
    let mut child = Command::new("xclip")
        .args(["-selection", "clipboard"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("xclip spawn failed: {}", e))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| format!("xclip stdin write failed: {}", e))?;
        // stdin drops here, sending EOF so xclip can acquire the selection
    }

    let status = child
        .wait()
        .map_err(|e| format!("xclip wait failed: {}", e))?;

    if !status.success() {
        return Err(format!("xclip exited with status: {}", status));
    }

    // 3. Small delay to let the clipboard settle
    thread::sleep(Duration::from_millis(30));

    // 4. Simulate Ctrl+V via xdotool (single key combo, instant)
    let paste_status = Command::new("xdotool")
        .args(["key", "--clearmodifiers", "ctrl+v"])
        .status()
        .map_err(|e| format!("xdotool key exec failed: {}", e))?;

    if !paste_status.success() {
        return Err(format!("xdotool key exited with status: {}", paste_status));
    }

    // 5. Wait for the target application to read from clipboard
    thread::sleep(Duration::from_millis(150));

    // 6. Restore previous clipboard contents (best-effort)
    if let Some(prev) = previous {
        if let Ok(mut restore) = Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            if let Some(mut stdin) = restore.stdin.take() {
                let _ = stdin.write_all(prev.as_bytes());
            }
            let _ = restore.wait();
        }
    }

    Ok(())
}

/// Type text using `ydotool type`.
/// Works on both X11 and Wayland. Requires ydotoold daemon running
/// and user in the `input` group (for /dev/uinput access).
#[cfg(target_os = "linux")]
fn type_via_ydotool(text: &str) -> Result<(), String> {
    let status = Command::new("ydotool")
        .arg("type")
        .arg("--")
        .arg(text)
        .status()
        .map_err(|e| format!("ydotool exec failed: {}", e))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("ydotool exited with status: {}", status))
    }
}

/// Type text using `wtype`.
/// Works on Wayland compositors that support the virtual-keyboard-unstable-v1
/// protocol (sway, river, and other wlroots-based compositors).
/// Does NOT work on GNOME/Mutter.
#[cfg(target_os = "linux")]
fn type_via_wtype(text: &str) -> Result<(), String> {
    let status = Command::new("wtype")
        .arg("--")
        .arg(text)
        .status()
        .map_err(|e| format!("wtype exec failed: {}", e))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("wtype exited with status: {}", status))
    }
}

/// Capture the currently focused window ID and name.
///
/// With cross-platform clipboard paste we do not rely on a window ID,
/// but keep the API for compatibility with the rest of the app.
pub fn capture_focused_window() -> Result<String, String> {
    Ok("active".to_string())
}

/// Insert text into the focused window.
///
/// Platform strategy:
///
/// **Linux (Wayland)**:
///   1. ydotool type  (uses /dev/uinput, works everywhere)
///   2. wtype         (wlroots virtual-keyboard protocol)
///   3. clipboard paste fallback
///
/// **Linux (X11)**:
///   1. xclip + xdotool key ctrl+v  (clipboard paste via CLI, instant)
///   2. arboard + enigo ctrl+v      (clipboard paste via libraries)
///
/// **macOS / Windows**:
///   1. enigo.text()  (native input methods, wrapped in catch_unwind)
///   2. clipboard paste fallback
pub fn type_text(text: &str, _target_window_id: &str) -> Result<(), String> {
    if text.is_empty() {
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        return type_text_linux(text);
    }

    #[cfg(not(target_os = "linux"))]
    {
        return type_text_nonlinux(text);
    }
}

/// Linux text insertion: process-based tools first, clipboard fallback last.
#[cfg(target_os = "linux")]
fn type_text_linux(text: &str) -> Result<(), String> {
    let wayland = is_wayland();
    log::info!(
        "type_text_linux: session={}, text='{}'",
        if wayland { "wayland" } else { "x11" },
        text
    );

    if wayland {
        // Wayland tier: ydotool -> wtype -> clipboard
        if command_exists("ydotool") {
            match type_via_ydotool(text) {
                Ok(()) => {
                    log::debug!("ydotool succeeded");
                    return Ok(());
                }
                Err(e) => log::warn!("ydotool failed: {}", e),
            }
        }

        if command_exists("wtype") {
            match type_via_wtype(text) {
                Ok(()) => {
                    log::debug!("wtype succeeded");
                    return Ok(());
                }
                Err(e) => log::warn!("wtype failed: {}", e),
            }
        }

        log::warn!("No Wayland typing tool available, falling back to clipboard paste");
    } else {
        // X11 tier: xclip+xdotool paste (fast) -> arboard clipboard paste
        if command_exists("xclip") && command_exists("xdotool") {
            match type_via_xclip_paste(text) {
                Ok(()) => {
                    log::debug!("xclip+xdotool paste succeeded");
                    return Ok(());
                }
                Err(e) => log::warn!("xclip+xdotool paste failed: {}", e),
            }
        }
    }

    // Final fallback for both X11 and Wayland
    log::info!("Falling back to clipboard paste");
    type_text_clipboard(text)
}

/// Non-Linux (macOS, Windows): enigo.text() first, clipboard fallback.
#[cfg(not(target_os = "linux"))]
fn type_text_nonlinux(text: &str) -> Result<(), String> {
    match type_text_enigo(text) {
        Ok(()) => {
            log::debug!("enigo.text() succeeded");
            return Ok(());
        }
        Err(e) => {
            log::warn!("enigo.text() failed ({}), falling back to clipboard paste", e);
        }
    }

    type_text_clipboard(text)
}

/// Use enigo.text() for direct keystroke input, wrapped in catch_unwind
/// to survive internal panics (enigo has .unwrap() calls inside).
/// Used on macOS/Windows as primary method; not used on Linux X11
/// (where xclip+xdotool clipboard paste is faster and more reliable).
#[allow(dead_code)]
fn type_text_enigo(text: &str) -> Result<(), String> {
    let text_owned = text.to_string();

    let result = catch_unwind(AssertUnwindSafe(|| -> Result<(), String> {
        let mut enigo = Enigo::new(&Settings::default())
            .map_err(|e| format!("Enigo init failed: {:?}", e))?;
        enigo
            .text(&text_owned)
            .map_err(|e| format!("enigo.text() failed: {:?}", e))?;
        Ok(())
    }));

    match result {
        Ok(inner) => inner,
        Err(panic_info) => {
            let msg = if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else if let Some(s) = panic_info.downcast_ref::<&str>() {
                s.to_string()
            } else {
                "unknown panic in enigo".to_string()
            };
            Err(format!("enigo panicked: {}", msg))
        }
    }
}

/// Fallback method: set clipboard then simulate Ctrl+V / Cmd+V.
///
/// Includes proper delays to avoid the race condition where the target
/// application hasn't read the clipboard before we restore the old content.
fn type_text_clipboard(text: &str) -> Result<(), String> {
    let mut clipboard = Clipboard::new().map_err(|e| format!("Clipboard init failed: {}", e))?;

    // Save previous clipboard contents (text only; images will be lost)
    let previous = clipboard.get_text().ok();

    // Set clipboard to the text we want to paste
    clipboard
        .set_text(text.to_string())
        .map_err(|e| format!("Failed to set clipboard: {}", e))?;

    // Small delay to let the clipboard settle before simulating paste.
    // Some compositors/apps need time to register the new clipboard content.
    thread::sleep(Duration::from_millis(30));

    // Simulate paste shortcut
    simulate_paste()?;

    // Delay to allow the target application to read from the clipboard.
    // This is the CRITICAL fix: without this delay, restoring the old clipboard
    // content races with the target app's paste read.
    thread::sleep(Duration::from_millis(150));

    // Restore previous clipboard contents
    if let Some(prev) = previous {
        // Don't fail the entire operation if restore fails
        if let Err(e) = clipboard.set_text(prev) {
            log::warn!("Failed to restore previous clipboard: {}", e);
        }
    }

    Ok(())
}

/// Simulate the platform-specific paste keyboard shortcut.
/// Wrapped in catch_unwind to handle enigo internal panics.
fn simulate_paste() -> Result<(), String> {
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<(), String> {
        let mut enigo = Enigo::new(&Settings::default())
            .map_err(|e| format!("Enigo init failed: {:?}", e))?;

        #[cfg(target_os = "macos")]
        {
            enigo
                .key(Key::Meta, Direction::Press)
                .map_err(|e| format!("Failed to press Meta: {:?}", e))?;
            enigo
                .key(Key::Unicode('v'), Direction::Click)
                .map_err(|e| format!("Failed to click 'v': {:?}", e))?;
            enigo
                .key(Key::Meta, Direction::Release)
                .map_err(|e| format!("Failed to release Meta: {:?}", e))?;
        }

        #[cfg(not(target_os = "macos"))]
        {
            enigo
                .key(Key::Control, Direction::Press)
                .map_err(|e| format!("Failed to press Control: {:?}", e))?;
            enigo
                .key(Key::Unicode('v'), Direction::Click)
                .map_err(|e| format!("Failed to click 'v': {:?}", e))?;
            enigo
                .key(Key::Control, Direction::Release)
                .map_err(|e| format!("Failed to release Control: {:?}", e))?;
        }

        Ok(())
    }));

    match result {
        Ok(inner) => inner,
        Err(panic_info) => {
            let msg = if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else if let Some(s) = panic_info.downcast_ref::<&str>() {
                s.to_string()
            } else {
                "unknown panic in enigo".to_string()
            };
            Err(format!("enigo panicked during paste simulation: {}", msg))
        }
    }
}

