//! Linux X11 window property setup.
//!
//! Sets WM_CLASS and _NET_WM_ICON on the standalone window so that desktop
//! environments display the correct application name and icon instead of the
//! generic "Unknown" fallback.

use std::thread;
use std::time::Duration;

use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::wrapper::ConnectionExt as WrapperConnectionExt;

const ICON_PNG: &[u8] = include_bytes!("../media/icon.png");
const WM_CLASS_VALUE: &[u8] = b"songwalker\0SongWalker\0";
const WINDOW_NAME: &str = "SongWalker";

/// Spawn a background thread that waits for the X11 window to appear and then
/// sets WM_CLASS (for taskbar name) and _NET_WM_ICON (for taskbar icon).
pub fn spawn_x11_setup_thread() {
    thread::spawn(|| {
        if let Err(e) = try_set_properties() {
            eprintln!("[x11_setup] failed to set window properties: {e}");
        }
    });
}

fn try_set_properties() -> Result<(), Box<dyn std::error::Error>> {
    let (conn, screen_num) = x11rb::connect(None)?;
    let screen = &conn.setup().roots[screen_num];

    // Poll for the window to appear (up to ~5 seconds)
    let mut window_id = None;
    for _ in 0..50 {
        thread::sleep(Duration::from_millis(100));
        if let Some(wid) = find_window_by_name(&conn, screen.root, WINDOW_NAME)? {
            window_id = Some(wid);
            break;
        }
    }

    let wid = match window_id {
        Some(w) => w,
        None => {
            eprintln!("[x11_setup] could not find window named '{WINDOW_NAME}'");
            return Ok(());
        }
    };

    // ── Set WM_CLASS ──
    conn.change_property8(
        PropMode::REPLACE,
        wid,
        AtomEnum::WM_CLASS,
        AtomEnum::STRING,
        WM_CLASS_VALUE,
    )?;

    // ── Set _NET_WM_ICON ──
    if let Err(e) = set_net_wm_icon(&conn, wid) {
        eprintln!("[x11_setup] failed to set _NET_WM_ICON: {e}");
    }

    conn.flush()?;
    Ok(())
}

/// Recursively search the window tree for a window whose WM_NAME matches `name`.
fn find_window_by_name(
    conn: &impl Connection,
    root: Window,
    name: &str,
) -> Result<Option<Window>, Box<dyn std::error::Error>> {
    let reply = conn.query_tree(root)?.reply()?;

    for child in reply.children {
        // Check WM_NAME
        let prop = conn
            .get_property(false, child, AtomEnum::WM_NAME, AtomEnum::STRING, 0, 1024)?
            .reply()?;

        if let Some(value) = std::str::from_utf8(&prop.value).ok() {
            if value == name {
                return Ok(Some(child));
            }
        }

        // Recurse into children
        if let Some(wid) = find_window_by_name(conn, child, name)? {
            return Ok(Some(wid));
        }
    }

    Ok(None)
}

/// Set _NET_WM_ICON from the embedded PNG.
///
/// The property format is an array of u32 values:
///   width, height, ARGB pixels..., [width, height, ARGB pixels...], ...
fn set_net_wm_icon(
    conn: &impl Connection,
    wid: Window,
) -> Result<(), Box<dyn std::error::Error>> {
    let img = image::load_from_memory_with_format(ICON_PNG, image::ImageFormat::Png)?;

    // Scale down for the icon — 48×48 is a good taskbar size
    let img = img.resize_exact(48, 48, image::imageops::FilterType::Lanczos3);
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();

    // Build the _NET_WM_ICON data: [width, height, ARGB pixels...]
    let mut data: Vec<u32> = Vec::with_capacity(2 + (w * h) as usize);
    data.push(w);
    data.push(h);

    for pixel in rgba.pixels() {
        let [r, g, b, a] = pixel.0;
        // Pack as ARGB (native u32)
        data.push(((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32));
    }

    // Intern the _NET_WM_ICON atom
    let atom_reply = conn
        .intern_atom(false, b"_NET_WM_ICON")?
        .reply()?;
    let cardinal_reply = conn
        .intern_atom(false, b"CARDINAL")?
        .reply()?;

    conn.change_property32(
        PropMode::REPLACE,
        wid,
        atom_reply.atom,
        cardinal_reply.atom,
        &data,
    )?;

    Ok(())
}
