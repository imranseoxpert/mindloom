//! macOS: files opened from Finder (double-click, Open With, dropping on the Dock icon) arrive as an
//! `application:openURLs:` message to the application delegate, which winit does not handle.
//! This adds that method (inherited by winit's delegate) and queues the paths for the app.

use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
use objc2::{ffi, sel};
use objc2_foundation::{NSArray, NSURL};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

static OPENED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static CTX: OnceLock<egui::Context> = OnceLock::new();

type OpenUrls = unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject, *mut NSArray<NSURL>);

unsafe extern "C-unwind" fn open_urls(_this: *mut AnyObject, _cmd: Sel, _app: *mut AnyObject, urls: *mut NSArray<NSURL>) {
    // SAFETY: AppKit passes a valid NSArray of NSURL objects that lives for the duration of this call.
    let Some(urls) = (unsafe { urls.as_ref() }) else { return };
    if let Ok(mut list) = OPENED.lock() {
        for u in urls.iter() {
            if let Some(p) = u.path() {
                list.push(PathBuf::from(p.to_string()));
            }
        }
    }
    if let Some(c) = CTX.get() {
        c.request_repaint();
    }
}

/// Start receiving files opened from Finder. Call once in `main`, before the event loop starts: a file
/// double-clicked while the app is not running arrives during launch, before any window exists.
///
/// winit creates its application delegate class while the event loop is built, so the method is added
/// to NSObject, which that class inherits from. AppKit sends `application:openURLs:` only to the
/// application's delegate, so other objects never receive it.
pub fn install() {
    let Some(cls) = AnyClass::get(c"NSObject") else { return };
    let f: OpenUrls = open_urls;
    // SAFETY: `open_urls` has the signature of `-application:openURLs:` ("v@:@@": void, self, _cmd,
    // NSApplication*, NSArray*). NSObject has no method of that name, and if one existed
    // class_addMethod would leave it unchanged and return NO.
    unsafe {
        let imp: Imp = std::mem::transmute::<OpenUrls, Imp>(f);
        ffi::class_addMethod(cls as *const AnyClass as *mut AnyClass, sel!(application:openURLs:), imp, c"v@:@@".as_ptr());
    }
}

/// Wake the interface when a file arrives.
pub fn set_context(ctx: &egui::Context) {
    let _ = CTX.set(ctx.clone());
}

/// Files opened from Finder since the last call.
pub fn take() -> Vec<PathBuf> {
    OPENED.lock().map(|mut l| std::mem::take(&mut *l)).unwrap_or_default()
}
