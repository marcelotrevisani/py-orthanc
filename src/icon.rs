//! Orthanc Tower icon generator.
//!
//! Renders a stylized Tower of Orthanc (from Lord of the Rings) on a rounded
//! Python-blue badge: a pale stone spire with forked prongs cradling a glowing
//! palantir in Python yellow.

use iced::window;

/// Create an iced window icon from the Orthanc tower.
pub fn window_icon() -> Option<window::Icon> {
    let size = 64u32;
    let rgba = orthanc_icon_rgba(size);
    window::icon::from_rgba(rgba, size, size).ok()
}

/// Linux app id. Wayland compositors (GNOME, KDE) ignore the per-window icon
/// and take it from the `.desktop` file whose basename matches this id.
#[cfg(target_os = "linux")]
const APP_ID: &str = "orthanc";

pub fn platform_specific() -> window::settings::PlatformSpecific {
    #[cfg(target_os = "linux")]
    {
        window::settings::PlatformSpecific {
            application_id: APP_ID.into(),
            ..Default::default()
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        Default::default()
    }
}

/// Register `orthanc.desktop` + icon under `~/.local/share` so Wayland shows
/// the icon for an unpackaged binary. Skipped when a package already installed
/// one system-wide; rewritten only when the content (e.g. exe path) changes.
#[cfg(target_os = "linux")]
pub fn install_desktop_entry() {
    let system = ["/usr/share/applications", "/usr/local/share/applications"];
    if system.iter().any(|d| {
        std::path::Path::new(d)
            .join(format!("{APP_ID}.desktop"))
            .exists()
    }) {
        return;
    }
    let (Some(data), Ok(exe)) = (dirs::data_dir(), std::env::current_exe()) else {
        return;
    };
    // Absolute icon path: a name lookup goes through the hicolor
    // icon-theme.cache, which won't list a file added after it was built.
    let icon = data.join(format!("icons/hicolor/256x256/apps/{APP_ID}.png"));
    let desktop = format!(
        "[Desktop Entry]\nType=Application\nName=Orthanc\nComment=Python environment management\n\
         Exec=\"{}\"\nIcon={}\nStartupWMClass={APP_ID}\nCategories=Development;\nTerminal=false\n",
        exe.display(),
        icon.display()
    );
    let files: [(std::path::PathBuf, &[u8]); 2] = [
        (icon, include_bytes!("../assets/icon-256.png")),
        (
            data.join(format!("applications/{APP_ID}.desktop")),
            desktop.as_bytes(),
        ),
    ];
    for (path, content) in files {
        if std::fs::read(&path).is_ok_and(|old| old == content) {
            continue;
        }
        let result = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|_| std::fs::write(&path, content));
        if let Err(e) = result {
            log::warn!("Could not write {}: {}", path.display(), e);
        }
    }
}

/// Generate the Orthanc tower icon as RGBA pixel data at the given size.
pub fn orthanc_icon_rgba(size: u32) -> Vec<u8> {
    let s = size as f32;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);

    // Normalized coordinates: everything relative to 0.0..1.0
    let cx = 0.5; // center x

    for py in 0..size {
        for px in 0..size {
            let x = px as f32 / s;
            let y = py as f32 / s;

            let (r, g, b, a) = orthanc_pixel(x, y, cx, s);
            rgba.push(r);
            rgba.push(g);
            rgba.push(b);
            rgba.push(a);
        }
    }

    rgba
}

/// Generate the icon as ARGB data (for ksni/D-Bus StatusNotifierItem).
#[cfg(target_os = "linux")]
pub fn orthanc_icon_argb(size: u32) -> Vec<u8> {
    let s = size as f32;
    let cx = 0.5;
    let mut data = Vec::with_capacity((size * size * 4) as usize);

    for py in 0..size {
        for px in 0..size {
            let x = px as f32 / s;
            let y = py as f32 / s;

            let (r, g, b, a) = orthanc_pixel(x, y, cx, s);
            data.push(a); // ARGB order
            data.push(r);
            data.push(g);
            data.push(b);
        }
    }

    data
}

/// Compute the color of a single pixel in normalized coordinates.
/// x, y in 0.0..1.0, cx = center x, s = pixel size for anti-aliasing.
///
/// Layers (back to front): rounded badge in Python blue so the icon reads on
/// both light and dark trays, palantir halo, pale stone tower, glowing orb.
fn orthanc_pixel(x: f32, y: f32, cx: f32, s: f32) -> (u8, u8, u8, u8) {
    let aa = 1.0 / s;
    let mut px = [0.0f32; 4]; // premultiplied-free RGBA, 0..1

    // ── Badge: rounded square, vertical gradient ──
    let badge = 1.0 - smooth_step(rounded_box_sdf(x - cx, y - 0.5, 0.47, 0.47, 0.2), -aa, aa);
    let top = [0.27, 0.49, 0.78];
    let bot = [0.09, 0.17, 0.35];
    over(&mut px, lerp3(top, bot, y), badge);

    // ── Palantir halo behind the tower top ──
    let (orb_x, orb_y) = (cx, 0.30);
    let dist = ((x - orb_x).powi(2) + (y - orb_y).powi(2)).sqrt();
    let halo = (1.0 - smooth_step(dist, 0.0, 0.2)) * 0.55 * badge;
    over(&mut px, [1.0, 0.83, 0.23], halo);

    // ── Tower: tapered body, base platform, two horned prongs ──
    let body = if (0.34..=0.84).contains(&y) {
        let hw = 0.085 + 0.085 * (y - 0.34) / 0.5;
        smooth_box(x, cx, hw, aa)
    } else {
        0.0
    };
    let platform = smooth_box(x, cx, 0.25, aa)
        * smooth_step(y, 0.82 - aa, 0.82 + aa)
        * (1.0 - smooth_step(y, 0.88 - aa, 0.88 + aa));
    let prong = |pcx: f32| -> f32 {
        // Straight shaft with a sharp tip from y=0.10 to 0.18.
        if !(0.10..=0.36).contains(&y) {
            return 0.0;
        }
        let hw = if y < 0.18 {
            0.04 * (y - 0.10) / 0.08
        } else {
            0.04
        };
        smooth_box(x, pcx, hw, aa)
    };
    let prongs = prong(cx - 0.085).max(prong(cx + 0.085));
    let tower = body.max(platform).max(prongs) * badge;

    // Lit from the left: left half brighter than the right.
    let shade = if x < cx { 1.0 } else { 0.8 };
    let stone = lerp3([0.95, 0.96, 0.99], [0.74, 0.78, 0.86], y).map(|c| c * shade);
    over(&mut px, stone, tower);

    // ── Palantir orb sitting between the prongs ──
    let orb = 1.0 - smooth_step(dist, 0.07 - aa, 0.07 + aa);
    let core = 1.0 - smooth_step(dist, 0.0, 0.065);
    over(
        &mut px,
        lerp3([1.0, 0.72, 0.1], [1.0, 0.98, 0.85], 1.0 - core),
        orb,
    );

    let to_u8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    (to_u8(px[0]), to_u8(px[1]), to_u8(px[2]), to_u8(px[3]))
}

/// Alpha-composite `color` with coverage `a` over `dst` (straight alpha).
fn over(dst: &mut [f32; 4], color: [f32; 3], a: f32) {
    let out_a = a + dst[3] * (1.0 - a);
    if out_a <= 0.0 {
        return;
    }
    for i in 0..3 {
        dst[i] = (color[i] * a + dst[i] * dst[3] * (1.0 - a)) / out_a;
    }
    dst[3] = out_a;
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

/// Signed distance to a rounded rectangle centered at the origin.
fn rounded_box_sdf(dx: f32, dy: f32, hw: f32, hh: f32, r: f32) -> f32 {
    let qx = dx.abs() - (hw - r);
    let qy = dy.abs() - (hh - r);
    (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r
}

/// Smooth step function: 0 at edge0, 1 at edge1, smooth transition.
fn smooth_step(x: f32, edge0: f32, edge1: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How much a point x is inside a box centered at cx with half-width hw.
/// Returns 0.0 outside, 1.0 inside, smooth transition at edges.
fn smooth_box(x: f32, cx: f32, hw: f32, aa: f32) -> f32 {
    let d = (x - cx).abs();
    1.0 - smooth_step(d, hw - aa, hw + aa)
}

#[cfg(test)]
mod preview {
    #[test]
    #[ignore]
    fn dump_png_preview() {
        let size: u32 = std::env::var("ICON_SIZE").map_or(256, |v| v.parse().unwrap());
        let rgba = super::orthanc_icon_rgba(size);
        let mut out = format!(
            "P7\nWIDTH {size}\nHEIGHT {size}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n"
        )
        .into_bytes();
        out.extend(rgba);
        std::fs::write(std::env::var("ICON_OUT").unwrap(), out).unwrap();
    }
}
