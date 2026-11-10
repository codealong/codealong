//! Apps on macOS, through AppKit: the running ones by bundle ID, Codealong's own (hidden, its Dock
//! icon, the links macOS opens with it), and an app's icon as the Finder shows it.

use std::path::Path;

use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSApplicationActivationOptions, NSRunningApplication};
use objc2_foundation::NSString;

/// Whether the app with this bundle ID is the active one, the app whose menus are in the menu bar.
/// Asked afresh each time, from any thread: a running app's record is as it was when it was
/// fetched, so each ask fetches it again.
pub fn is_active(bundle_id: &str) -> bool {
    let apps = NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(bundle_id));
    apps.count() > 0 && apps.objectAtIndex(0).isActive()
}

/// Asks the running app with this bundle ID to come forward. On the main thread.
pub fn activate(bundle_id: &str) {
    let apps = NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(bundle_id));
    if apps.count() > 0 {
        #[allow(deprecated)]
        let _ = apps.objectAtIndex(0).activateWithOptions(NSApplicationActivationOptions::empty());
    }
}

/// The app in front, by its bundle ID, when it isn't Codealong and has one.
pub fn frontmost_other() -> Option<String> {
    let id = objc2_app_kit::NSWorkspace::sharedWorkspace().frontmostApplication()?.bundleIdentifier()?.to_string();
    let own = NSRunningApplication::currentApplication().bundleIdentifier().map(|i| i.to_string());
    (own.as_deref() != Some(id.as_str())).then_some(id)
}

/// Hides Codealong, as ⌘H does: its board windows go, the panels stay (panel.rs `place`).
pub fn hide(app: &tauri::AppHandle) -> bool {
    let _ = app.hide();
    true
}

/// Whether macOS's LaunchServices opened this process: it knows the copy it opened from its first
/// moment, before it has said it's running, and nothing else (install.rs).
pub fn opened_by_macos(pid: u32) -> bool {
    NSRunningApplication::runningApplicationWithProcessIdentifier(pid as libc::pid_t).is_some()
}

/// The running app's icon in the Dock, from a PNG: the bundled idle icon stays neutral, and only
/// the running application's icon changes.
pub fn set_dock_icon(app: &tauri::AppHandle, png: &'static [u8]) -> tauri::Result<()> {
    use objc2::AnyThread;
    use objc2_app_kit::NSImage;
    use objc2_foundation::NSData;
    app.run_on_main_thread(move || {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let data = NSData::with_bytes(png);
        if let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) {
            // SAFETY: AppKit requires the main thread, which the marker says this is, and
            // the image is one that decoded.
            unsafe { NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&image)); }
        } else {
            eprintln!("Couldn't decode the Codealong status icon.");
        }
    })
}

/// An app's icon as the Finder shows it, `px` square, as straight RGBA, top row first.
pub fn icon(app: &Path, px: usize) -> Option<Vec<u8>> {
    use objc2::AllocAnyThread;
    use objc2_app_kit::{NSBitmapImageRep, NSCompositingOperation, NSDeviceRGBColorSpace, NSGraphicsContext, NSImageInterpolation, NSWorkspace};
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
    let side = px as isize;
    let icon = NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(&app.to_string_lossy()));
    // 8 bits a sample, RGBA, rows packed: the layout `pixels` reads.
    // SAFETY: with no planes given AppKit allocates the pixels itself and owns them, sized by the
    // numbers here, which agree with each other (4 samples of 8 bits are the 32 bits of a pixel
    // and the `px * 4` bytes of a row); the color space's name is AppKit's constant.
    let rep = unsafe { NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(NSBitmapImageRep::alloc(), std::ptr::null_mut(), side, side, 8, 4, true, false, NSDeviceRGBColorSpace, side * 4, 32) }?;
    let ctx = NSGraphicsContext::graphicsContextWithBitmapImageRep(&rep)?;
    NSGraphicsContext::saveGraphicsState_class();
    NSGraphicsContext::setCurrentContext(Some(&ctx));
    ctx.setImageInterpolation(NSImageInterpolation::High);
    let all = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(px as f64, px as f64));
    // An empty source rect draws the whole image, from the representation that best fits.
    icon.drawInRect_fromRect_operation_fraction(all, NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0)), NSCompositingOperation::Copy, 1.0);
    ctx.flushGraphics();
    NSGraphicsContext::restoreGraphicsState_class();
    let data = rep.bitmapData();
    if data.is_null() || rep.bytesPerRow() != side * 4 {
        return None;
    }
    // SAFETY: the pixels aren't null and their rows are `px * 4` bytes, both just checked, and
    // there are `px` rows, as asked for above: that many bytes are the rep's, which is alive until
    // this returns and which nothing draws into again; `straight` copies them out.
    let premultiplied = unsafe { std::slice::from_raw_parts(data, px * px * 4) };
    Some(straight(premultiplied))
}

/// The links macOS asked Codealong to open: a click, in another app, on a link of a scheme the
/// app's Info.plist claims (`codealong://…`). They come as an event of the app's, ahead of the
/// one that says it finished launching when the click is what launched it.
pub fn opened_links(event: &tauri::RunEvent) -> Vec<String> {
    match event {
        tauri::RunEvent::Opened { urls } => urls.iter().map(|u| u.to_string()).collect(),
        _ => Vec::new(),
    }
}

/// Has macOS open `scheme`'s links with the app at `bundle` from now on, whichever other app
/// claims the scheme too. LaunchServices keeps the choice by the app's bundle ID, through that
/// app being replaced and the other one registering again, asks nobody for a scheme that's an
/// app's own (it does for the web's and mail's), and answers at once
/// (`.claude/rules/windows.md`). What goes wrong is only said: the links then open in whichever
/// app macOS picks, the one registered last.
pub fn claim_links(bundle: &Path, scheme: &str) {
    let app = objc2_foundation::NSURL::fileURLWithPath(&NSString::from_str(&bundle.to_string_lossy()));
    let said = scheme.to_string();
    let done = block2::RcBlock::new(move |error: *mut objc2_foundation::NSError| {
        // SAFETY: AppKit hands the completion an NSError it owns for the call, or null.
        if let Some(e) = unsafe { error.as_ref() } {
            eprintln!("Couldn't have macOS open {said}:// links here: {}", e.localizedDescription());
        }
    });
    objc2_app_kit::NSWorkspace::sharedWorkspace().setDefaultApplicationAtURL_toOpenURLsWithScheme_completionHandler(&app, &NSString::from_str(scheme), Some(&done));
}

/// Premultiplied RGBA, as AppKit draws, made straight, as menus and canvases take it.
fn straight(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks_exact(4)
        .flat_map(|p| {
            let a = p[3] as u32;
            let un = |c: u8| if a == 0 { 0 } else { ((c as u32 * 255 + a / 2) / a).min(255) as u8 };
            [un(p[0]), un(p[1]), un(p[2]), p[3]]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpremultiplies_what_appkit_draws() {
        // Opaque stays, clear goes to zero, half-covered comes back to its color.
        assert_eq!(straight(&[10, 20, 30, 255, 5, 5, 5, 0, 64, 32, 0, 128]), [10, 20, 30, 255, 0, 0, 0, 0, 128, 64, 0, 128]);
    }
}
