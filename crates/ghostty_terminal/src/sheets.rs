//! Native sheets the Ghostty fork shows with `NSAlert`.

use std::cell::RefCell;

use block::ConcreteBlock;
use cocoa::{
    base::id,
    foundation::{NSPoint, NSRect, NSSize},
};
use futures::channel::oneshot;
use objc::{class, msg_send, sel, sel_impl};

use crate::{ns_string, ns_string_to_string};

const NS_ALERT_FIRST_BUTTON_RETURN: i64 = 1000;

/// What a clipboard confirmation is about, the Ghostty app's
/// `ClipboardRequest`.
pub enum ClipboardRequest {
    Paste,
    Read,
    Write,
}

impl ClipboardRequest {
    fn title(&self) -> &'static str {
        match self {
            ClipboardRequest::Paste => "Warning: Potentially Unsafe Paste",
            ClipboardRequest::Read | ClipboardRequest::Write => "Authorize Clipboard Access",
        }
    }

    fn message(&self) -> &'static str {
        match self {
            ClipboardRequest::Paste => {
                "Pasting this text to the terminal may be dangerous as it looks like some commands may be executed."
            }
            ClipboardRequest::Read => {
                "An application is attempting to read from the clipboard.\nThe current clipboard contents are shown below."
            }
            ClipboardRequest::Write => {
                "An application is attempting to write to the clipboard.\nThe content to write is shown below."
            }
        }
    }

    fn buttons(&self) -> (&'static str, &'static str) {
        match self {
            ClipboardRequest::Paste => ("Paste", "Cancel"),
            ClipboardRequest::Read | ClipboardRequest::Write => ("Allow", "Deny"),
        }
    }
}

/// The Ghostty app's clipboard confirmation: a warning with the contents in a
/// monospaced, read-only text view. Sends whether the user confirmed.
///
/// # Safety
///
/// `window` must be a live `NSWindow`; call on the main thread.
pub unsafe fn confirm_clipboard(
    window: id,
    request: ClipboardRequest,
    contents: &str,
    reply: oneshot::Sender<bool>,
) {
    unsafe {
        let alert: id = msg_send![class!(NSAlert), alloc];
        let alert: id = msg_send![alert, init];
        // NSAlertStyleWarning: the yellow triangle.
        let _: () = msg_send![alert, setAlertStyle: 0u64];
        let _: () = msg_send![alert, setMessageText: ns_string(request.title())];
        let _: () = msg_send![alert, setInformativeText: ns_string(request.message())];

        let frame = NSRect::new(NSPoint::new(0., 0.), NSSize::new(480., 200.));
        let scroll: id = msg_send![class!(NSScrollView), alloc];
        let scroll: id = msg_send![scroll, initWithFrame: frame];
        let _: () = msg_send![scroll, setHasVerticalScroller: true];
        let _: () = msg_send![scroll, setBorderType: 2u64];
        let text_view: id = msg_send![class!(NSTextView), alloc];
        let text_view: id = msg_send![text_view, initWithFrame: frame];
        let _: () = msg_send![text_view, setEditable: false];
        let font: id =
            msg_send![class!(NSFont), monospacedSystemFontOfSize: 12.0f64 weight: 0.0f64];
        let _: () = msg_send![text_view, setFont: font];
        let _: () = msg_send![text_view, setString: ns_string(contents)];
        let _: () = msg_send![scroll, setDocumentView: text_view];
        let _: () = msg_send![alert, setAccessoryView: scroll];

        let (confirm, cancel) = request.buttons();
        let _: id = msg_send![alert, addButtonWithTitle: ns_string(confirm)];
        let cancel_button: id = msg_send![alert, addButtonWithTitle: ns_string(cancel)];
        // Escape declines, like the app's `.cancelAction`.
        let _: () = msg_send![cancel_button, setKeyEquivalent: ns_string("\u{1b}")];

        let reply = RefCell::new(Some(reply));
        let handler = ConcreteBlock::new(move |response: i64| {
            if let Some(reply) = reply.borrow_mut().take() {
                reply.send(response == NS_ALERT_FIRST_BUTTON_RETURN).ok();
            }
            let _: () = msg_send![text_view, release];
            let _: () = msg_send![scroll, release];
            let _: () = msg_send![alert, release];
        });
        let handler = handler.copy();
        let _: () = msg_send![alert, beginSheetModalForWindow: window completionHandler: &*handler];
    }
}

/// Whether the 1Password CLI is installed and has been used on this machine:
/// the `op` binary where its installers put it, and the config directory it
/// writes on its first sign-in (`~/.config/op`, or `$OP_CONFIG_DIR`).
fn op_cli_in_use() -> bool {
    let installed = ["/usr/local/bin/op", "/opt/homebrew/bin/op"]
        .iter()
        .any(|p| std::path::Path::new(p).exists())
        || paths::home_dir().join(".local/bin/op").exists();
    let config = std::env::var_os("OP_CONFIG_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| paths::home_dir().join(".config/op"));
    installed && config.is_dir()
}

/// The Ghostty fork's Full Disk Access reminder, for the app the terminals now
/// run under: without it the 1Password CLI in them cannot reach the desktop
/// app. Shown as a sheet, at most once an hour.
///
/// # Safety
///
/// `window` must be a live `NSWindow`; call on the main thread.
pub unsafe fn remind_full_disk_access(window: id) {
    const LAST_PROMPT_KEY: &str = "FullDiskAccessLastPrompt";
    const PROMPT_INTERVAL_SECONDS: f64 = 3600.;
    const SETTINGS_URL: &str =
        "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles";

    // Only for someone who uses the 1Password CLI: everyone else has no use
    // for Full Disk Access here, and the reminder would be a riddle to them.
    if !op_cli_in_use() {
        return;
    }
    // Either TCC database opens only with Full Disk Access. The system one is
    // the probe: the per-user database is gone on macOS 26 and later, so
    // asking for it alone failed on a machine that had the access and the
    // reminder came up every hour.
    let probes = [
        std::path::PathBuf::from("/Library/Application Support/com.apple.TCC/TCC.db"),
        paths::home_dir().join("Library/Application Support/com.apple.TCC/TCC.db"),
    ];
    if probes
        .iter()
        .any(|probe| std::fs::File::open(probe).is_ok())
    {
        return;
    }
    unsafe {
        let defaults: id = msg_send![class!(NSUserDefaults), standardUserDefaults];
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs_f64())
            .unwrap_or_default();
        let last: f64 = msg_send![defaults, doubleForKey: ns_string(LAST_PROMPT_KEY)];
        if now - last <= PROMPT_INTERVAL_SECONDS {
            return;
        }
        let _: () = msg_send![defaults, setDouble: now forKey: ns_string(LAST_PROMPT_KEY)];

        let app_path: id = {
            let bundle: id = msg_send![class!(NSBundle), mainBundle];
            msg_send![bundle, bundlePath]
        };
        let app_path =
            ns_string_to_string(app_path).unwrap_or_else(|| "/Applications/Sinter.app".into());
        let alert: id = msg_send![class!(NSAlert), alloc];
        let alert: id = msg_send![alert, init];
        let _: () = msg_send![alert, setAlertStyle: 0u64];
        let _: () =
            msg_send![alert, setMessageText: ns_string("Sinter is missing Full Disk Access")];
        let text = format!(
            "Commands run in the terminal inherit Sinter's permissions. Without Full Disk Access \
             the 1Password CLI cannot reach the desktop app and reports \"No accounts configured\", \
             so anything resolving secrets through `op` fails: ansible-vault, SMB mounts, API tokens.\n\n\
             Add {app_path} under Full Disk Access. If it is already listed, remove it with the \
             minus (-) button and add it again: the entry is tied to the app's code signature. \
             Then quit Sinter with Cmd+Q and relaunch; a running process keeps the old grants."
        );
        let _: () = msg_send![alert, setInformativeText: ns_string(&text)];
        let _: id = msg_send![alert, addButtonWithTitle: ns_string("Open Settings")];
        let _: id = msg_send![alert, addButtonWithTitle: ns_string("Later")];
        let handler = ConcreteBlock::new(move |response: i64| {
            if response == NS_ALERT_FIRST_BUTTON_RETURN {
                let url: id = msg_send![class!(NSURL), URLWithString: ns_string(SETTINGS_URL)];
                let workspace: id = msg_send![class!(NSWorkspace), sharedWorkspace];
                let _: bool = msg_send![workspace, openURL: url];
            }
            let _: () = msg_send![alert, release];
        });
        let handler = handler.copy();
        let _: () = msg_send![alert, beginSheetModalForWindow: window completionHandler: &*handler];
    }
}
