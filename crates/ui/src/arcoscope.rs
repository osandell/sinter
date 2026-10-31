//! Arcoscope page tinting shared by the tab bar and the workspace's bottom strip.
//!
//! The palette and blend math are kept verbatim with the Ghostty fork
//! (`ArcoscopePageMonitor.accents` and `ThemedTabPalette` in `ZedTabBar.swift`) so
//! the terminal and the editor show the exact same color for a given arcoscope
//! page. Only the active (key) window picks up the page tint; inactive windows
//! stay on the theme's neutral background.

use gpui::{
    App, FocusHandle, Global, Hsla, InteractiveElement, MouseButton, Rgba, SharedString,
    WeakFocusHandle, Window, rgb,
};

/// The active arcoscope "page" (0-based), pushed from the arcoscope daemon over Zed's
/// CLI datagram socket. `None` = unknown.
#[derive(Default)]
pub struct ArcoscopePage(Option<usize>);

impl Global for ArcoscopePage {}

/// Whether this app is the frontmost one, set from NSWorkspace activations.
/// `window.is_window_active()` alone is not enough: a Zed window can stay key
/// while another app (the Ghostty fork) holds the front, and then both apps
/// painted the page tint at once. `None` = not heard yet, treated as frontmost.
#[derive(Default)]
struct ArcoscopeAppFront(Option<bool>);

impl Global for ArcoscopeAppFront {}

/// Record whether Zed is the frontmost app and redraw on a change, so only the
/// app the user is actually in shows the page color.
pub fn set_arcoscope_app_front(front: bool, cx: &mut App) {
    if cx.try_global::<ArcoscopeAppFront>().and_then(|f| f.0) == Some(front) {
        return;
    }
    cx.set_global(ArcoscopeAppFront(Some(front)));
    cx.refresh_windows();
}

/// Focus handles of the terminal columns sharing the window with the editor.
/// The window tints only the half holding the keyboard, so a glance at the
/// bars tells whether keys go to the terminal or the editor.
#[derive(Default)]
struct ArcoscopeTerminalFocus(Vec<WeakFocusHandle>);

impl Global for ArcoscopeTerminalFocus {}

/// Register a terminal column's focus handle; while focus is inside it the
/// editor's bars stay neutral and the terminal's take the page tint.
pub fn register_arcoscope_terminal_focus(handle: &FocusHandle, cx: &mut App) {
    let handles = &mut cx.default_global::<ArcoscopeTerminalFocus>().0;
    handles.retain(|handle| handle.upgrade().is_some());
    handles.push(handle.downgrade());
}

/// Whether the keyboard is in a terminal column of `window`.
pub fn arcoscope_terminal_focused(window: &Window, cx: &App) -> bool {
    cx.try_global::<ArcoscopeTerminalFocus>().is_some_and(|focus| {
        focus
            .0
            .iter()
            .filter_map(WeakFocusHandle::upgrade)
            .any(|handle| handle.contains_focused(window, cx))
    })
}

/// Base the bar tints from when the window is active, before the page accent is
/// blended in. One per appearance, matching `lightBars.barActive` and
/// `darkBars.barActive` in the Ghostty fork — a single light base left the bars
/// glowing pale against a dark editor while the terminal went dark blue/green.
const ARCOSCOPE_BAR_ACTIVE_LIGHT: u32 = 0xd5dce1;
const ARCOSCOPE_BAR_ACTIVE_DARK: u32 = 0x3c3836;

/// Relative luminance of `color`, 0 (black) to 1 (white).
///
/// The appearance is read off the neutral background we were handed rather than
/// from the theme, which is what the Ghostty fork does too (`bars(for:)` switches
/// on the terminal background's luminance). It also means a theme that is dark
/// without saying so still gets the dark base.
fn luminance(color: Hsla) -> f32 {
    let rgba: Rgba = color.into();
    0.2126 * rgba.r + 0.7152 * rgba.g + 0.0722 * rgba.b
}

/// The active-window base for the appearance implied by `neutral`.
fn bar_active_base(neutral: Hsla) -> u32 {
    if luminance(neutral) < 0.5 {
        ARCOSCOPE_BAR_ACTIVE_DARK
    } else {
        ARCOSCOPE_BAR_ACTIVE_LIGHT
    }
}

/// Fraction of the (dark) page accent blended into the light base. Kept
/// moderate so tab-label text stays readable (matches `pageTintAmount`).
const ARCOSCOPE_PAGE_TINT_AMOUNT: f32 = 0.30;

/// Dark accent color for each of arcoscope's six single-colour collections, in
/// its order (`COLLECTION_COLORS` in arcoscope-core): orange, green, purple,
/// blue, yellow, pink. Mirrors `ArcoscopePageMonitor.accents` in the Ghostty
/// fork.
const ARCOSCOPE_ACCENTS: [u32; 6] = [
    0xb55512, // dark orange
    0x3a5f2a, // green
    0x6e4f9e, // purple
    0x2e4f6b, // blue
    0x8a7a1e, // yellow
    0xa84a78, // pink
];

/// The collections past the first six carry two of those colours each:
/// orange and yellow, green and blue, purple and pink.
const ARCOSCOPE_PAIRS: [(usize, usize); 3] = [(0, 4), (1, 3), (2, 5)];

/// The accent (or two) of an arcoscope page, or `None` if out of range.
fn arcoscope_page_accents(page: usize) -> Option<(u32, Option<u32>)> {
    if let Some(accent) = ARCOSCOPE_ACCENTS.get(page) {
        return Some((*accent, None));
    }
    let (first, second) = ARCOSCOPE_PAIRS.get(page - ARCOSCOPE_ACCENTS.len())?;
    Some((
        *ARCOSCOPE_ACCENTS.get(*first)?,
        Some(*ARCOSCOPE_ACCENTS.get(*second)?),
    ))
}

/// The single-colour pages a page is drawn in, the lighter first: the page
/// itself twice for one of the first six. The skins' per-collection copies
/// exist for those only, so a two-colour page puts two of them side by side.
pub(crate) fn arcoscope_page_halves(page: usize) -> Option<(usize, usize)> {
    if page < ARCOSCOPE_ACCENTS.len() {
        return Some((page, page));
    }
    let (first, second) = *ARCOSCOPE_PAIRS.get(page - ARCOSCOPE_ACCENTS.len())?;
    let lum = |index: usize| luminance(rgb(ARCOSCOPE_ACCENTS[index]).into());
    Some(if lum(first) >= lum(second) { (first, second) } else { (second, first) })
}

/// The accent of one of the six single-colour pages.
pub(crate) fn arcoscope_single_accent(page: usize) -> Option<Hsla> {
    ARCOSCOPE_ACCENTS.get(page).map(|accent| rgb(*accent).into())
}

/// The single accent standing for a page: the colour itself, or the first of
/// a pair.
fn arcoscope_page_accent(page: usize) -> Option<u32> {
    arcoscope_page_accents(page).map(|(accent, _)| accent)
}

/// Channel-wise lerp of two packed `0xRRGGBB` colors: `amount` of `accent`
/// blended into `base` (matches `ThemedTabPalette.tint`).
fn tint(base: u32, accent: u32, amount: f32) -> Hsla {
    let channel = |shift: u32| {
        let b = ((base >> shift) & 0xff) as f32;
        let a = ((accent >> shift) & 0xff) as f32;
        ((b + (a - b) * amount).round() as u32) & 0xff
    };
    rgb((channel(16) << 16) | (channel(8) << 8) | channel(0)).into()
}

/// Background for the editor's tab bar / bottom strip.
///
/// Bars stay neutral when another app or a terminal column holds focus.
pub fn arcoscope_bar_background(window: &Window, neutral: Hsla, cx: &App) -> Hsla {
    if !editor_holds_arcoscope_focus(window, cx) {
        return neutral;
    }
    arcoscope_page_tint(neutral, cx)
}

/// Whether the editor side of this window is where the user is: the key window
/// of the frontmost app, with no terminal column holding focus. The editor's
/// bars carry the page colour only then; the terminal colours its own.
fn editor_holds_arcoscope_focus(window: &Window, cx: &App) -> bool {
    let app_front = cx
        .try_global::<ArcoscopeAppFront>()
        .and_then(|f| f.0)
        .unwrap_or(true);
    window.is_window_active() && app_front && !arcoscope_terminal_focused(window, cx)
}

/// The active base for the appearance implied by `neutral`, tinted toward the
/// current page's accent. For a two-colour page this is the darker of the two
/// tints, the right half of a bar (`arcoscope_page_glow` is the left).
pub fn arcoscope_page_tint(neutral: Hsla, cx: &App) -> Hsla {
    page_tints(neutral, cx).0
}

/// The lighter tint of a two-colour page, the left half of a bar
/// (`arcoscope_split_overlay`). `None` for a single-colour page.
pub fn arcoscope_page_glow(neutral: Hsla, cx: &App) -> Option<Hsla> {
    page_tints(neutral, cx).1
}

/// `arcoscope_page_glow` on the same terms as `arcoscope_bar_background`.
pub fn arcoscope_bar_glow(window: &Window, neutral: Hsla, cx: &App) -> Option<Hsla> {
    if !editor_holds_arcoscope_focus(window, cx) {
        return None;
    }
    arcoscope_page_glow(neutral, cx)
}

/// (darker, lighter) tints of the current page.
fn page_tints(neutral: Hsla, cx: &App) -> (Hsla, Option<Hsla>) {
    page_tints_of(neutral, arcoscope_page(cx))
}

/// (darker, lighter) tints of `page`.
fn page_tints_of(neutral: Hsla, page: Option<usize>) -> (Hsla, Option<Hsla>) {
    let base = bar_active_base(neutral);
    let Some((first, second)) = page.and_then(arcoscope_page_accents) else {
        return (rgb(base).into(), None);
    };
    let first = tint(base, first, ARCOSCOPE_PAGE_TINT_AMOUNT);
    let Some(second) = second else {
        return (first, None);
    };
    let second = tint(base, second, ARCOSCOPE_PAGE_TINT_AMOUNT);
    if luminance(first) >= luminance(second) {
        (second, Some(first))
    } else {
        (first, Some(second))
    }
}

/// A two-colour page over a bar: its lighter tint on the left half, the bar's
/// own (darker) tint left showing on the right, split by a short edge slanted
/// like `/` through the middle, the way arcoscope splits a collection key on
/// its diagonal. Absolute and behind content placed after it.
pub fn arcoscope_split_overlay(light: Hsla) -> gpui::Canvas<()> {
    use gpui::{Styled, canvas, fill, linear_color_stop, linear_gradient};
    canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            let width = f32::from(bounds.size.width);
            let height = f32::from(bounds.size.height);
            if width <= 0. || height <= 0. {
                return;
            }
            // The shader squeezes the gradient's direction by the box's aspect
            // ratio; this angle undoes that, so the edge runs at 45 degrees on
            // screen. Two stops one pixel apart give a hard edge (equal ones
            // would divide by zero in the shader).
            let angle = 90. + (width / height).atan().to_degrees();
            let step = 1. / width.max(height);
            let clear = Hsla { a: 0., ..light };
            window.paint_quad(fill(
                bounds,
                linear_gradient(
                    angle,
                    linear_color_stop(light, 0.5),
                    linear_color_stop(clear, 0.5 + step),
                ),
            ));
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// The collection (0-based) arcoscope parked on while it holds no scope, set by
/// `zed://arcoscope/empty/<n>`: the window shows the empty view for it
/// (`arcoscope_empty_view`) instead of the workspace until arcoscope shows a
/// worktree again.
#[derive(Default)]
struct ArcoscopeEmpty(Option<usize>);

impl Global for ArcoscopeEmpty {}

/// Enter (`Some`) or leave (`None`) the empty view, redrawing the windows on a
/// change.
pub fn set_arcoscope_empty(collection: Option<usize>, cx: &mut App) {
    if cx.try_global::<ArcoscopeEmpty>().map(|empty| empty.0) == Some(collection)
        || (collection.is_none() && cx.try_global::<ArcoscopeEmpty>().is_none())
    {
        return;
    }
    cx.set_global(ArcoscopeEmpty(collection));
    cx.refresh_windows();
}

pub fn arcoscope_empty(cx: &App) -> Option<usize> {
    cx.try_global::<ArcoscopeEmpty>().and_then(|empty| empty.0)
}

/// arcoscope's daemon socket, where its interface's verbs go.
fn arcoscope_daemon_socket() -> std::path::PathBuf {
    if cfg!(target_os = "macos") {
        return "/tmp/arcoscope.sock".into();
    }
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("arcoscope.sock")
}

/// arcoscope's project browser, as a click on an empty slot of its bar opens
/// it; the project picked there opens in the collection arcoscope is parked on.
fn open_arcoscope_project_browser(cx: &App) {
    use gpui::AppContext as _;
    cx.background_spawn(async move {
        use std::io::Write as _;
        let path = arcoscope_daemon_socket();
        if let Err(error) = std::os::unix::net::UnixStream::connect(&path)
            .and_then(|mut stream| stream.write_all(b"projects-show\n"))
        {
            log::error!("projects-show to {}: {error}", path.display());
        }
    })
    .detach();
}

/// The window while arcoscope is on an empty collection: no worktree, a line
/// saying so with a button that opens arcoscope's project browser, and the
/// collection's colour strip along the bottom.
pub fn arcoscope_empty_view(collection: usize, cx: &App) -> gpui::AnyElement {
    use crate::prelude::*;
    use crate::{Button, ButtonStyle, Label};
    let colors = cx.theme().colors();
    let strip = crate::arcoscope_skin_surface_variant(
        "bottom_strip",
        Some(collection),
        cx,
    )
    .unwrap_or_else(|| {
        // No skin: the drawn page colours, as the bars draw them.
        let (dark, light) = page_tints_of(colors.tab_bar_background, Some(collection));
        div()
            .absolute()
            .inset_0()
            .bg(dark)
            .children(light.map(arcoscope_split_overlay))
            .into_any_element()
    });
    v_flex()
        .id("arcoscope-empty")
        .size_full()
        .bg(colors.editor_background)
        .child(
            v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .gap_3()
                .child(
                    Label::new("No scope is open in this collection yet")
                        .color(Color::Muted),
                )
                .child(
                    Button::new("arcoscope-open-scope", "Open scope")
                        .style(ButtonStyle::Filled)
                        .on_click(|_, _, cx| open_arcoscope_project_browser(cx)),
                ),
        )
        .child(
            div()
                .relative()
                .w_full()
                .flex_none()
                .h(px(ARCOSCOPE_EMPTY_STRIP_HEIGHT))
                .child(strip),
        )
        .into_any_element()
}

/// The empty view's strip: as tall as the workspace's bottom strip.
const ARCOSCOPE_EMPTY_STRIP_HEIGHT: f32 = 10.0;

/// Update the active arcoscope page and redraw every window so the tint follows
/// it. No-op when the page is unchanged.
pub fn set_arcoscope_page(page: usize, cx: &mut App) {
    if cx.try_global::<ArcoscopePage>().map(|page| page.0) == Some(Some(page)) {
        return;
    }
    cx.set_global(ArcoscopePage(Some(page)));
    cx.refresh_windows();
}

#[derive(Default)]
pub struct ArcoscopeTheme {
    snapshot: Option<ThemeSnapshot>,
    missing_theme: Option<SharedString>,
}

impl Global for ArcoscopeTheme {}

#[derive(Clone, Debug, PartialEq)]
struct ThemeSnapshot {
    name: String,
    binding: Option<crate::arcoscope_skin::ThemeBinding>,
}

#[derive(serde::Deserialize)]
struct ThemeBindings {
    #[serde(default = "follow_themes_by_default")]
    enabled: bool,
    #[serde(default)]
    themes: std::collections::BTreeMap<String, crate::arcoscope_skin::ThemeBinding>,
}

fn follow_themes_by_default() -> bool {
    true
}

pub fn arcoscope_amiga(cx: &App) -> bool {
    cx.try_global::<ArcoscopeTheme>()
        .and_then(|state| state.snapshot.as_ref())
        .and_then(|snapshot| snapshot.binding.as_ref())
        .is_some_and(|binding| binding.chrome.as_deref() == Some("amiga") && binding.skin.is_none())
}

/// Keep terminal status icons in step with Arcoscope's non-flat themes.
pub fn arcoscope_pixel_art(cx: &App) -> bool {
    cx.try_global::<ArcoscopeTheme>()
        .and_then(|state| state.snapshot.as_ref())
        .is_some_and(|snapshot| {
            snapshot.name != "flat"
                && snapshot
                    .binding
                    .as_ref()
                    .is_some_and(|binding| binding.pixel_art.unwrap_or(true))
        })
}

/// The arcoscope bar theme whose mapping is applied ("mist", "gruvbox-dark", ...),
/// or `None` when no mapping is. Vector themes share one layout and differ only
/// in colour; callers that draw their own marks pick the palette with this.
pub fn arcoscope_bar_theme(cx: &App) -> Option<&str> {
    cx.try_global::<ArcoscopeTheme>()
        .and_then(|state| state.snapshot.as_ref())
        .filter(|snapshot| snapshot.binding.is_some())
        .map(|snapshot| snapshot.name.as_str())
}

/// Explicit theme selection for standalone terminal previews.
pub fn set_arcoscope_amiga(amiga: bool, cx: &mut App) {
    let settings = if amiga {
        r#"{"barTheme":"amiga"}"#
    } else {
        r#"{"barTheme":"flat"}"#
    };
    match theme_snapshot(Some(settings), None) {
        Ok(snapshot) => apply_arcoscope_theme(snapshot, cx),
        Err(error) => log::error!("Arcoscope theme configuration: {error}"),
    }
}

fn read_optional_file(path: &std::path::Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

fn theme_snapshot(
    settings: Option<&str>,
    overrides: Option<&str>,
) -> Result<ThemeSnapshot, String> {
    let mut bindings: ThemeBindings = serde_json::from_str(include_str!(
        "../../../assets/images/window-skins/arcoscope.json"
    ))
    .map_err(|error| error.to_string())?;
    if let Some(overrides) = overrides {
        let overrides: ThemeBindings =
            serde_json::from_str(overrides).map_err(|error| error.to_string())?;
        bindings.enabled = overrides.enabled;
        bindings.themes.extend(overrides.themes);
    }
    let settings: serde_json::Value = settings
        .map(serde_json::from_str)
        .transpose()
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    let name = settings
        .get("barTheme")
        .and_then(|value| value.as_str())
        .unwrap_or("flat")
        .to_owned();
    let binding = if bindings.enabled {
        bindings.themes.remove(&name)
    } else {
        None
    };
    Ok(ThemeSnapshot { name, binding })
}

fn read_arcoscope_theme() -> Result<ThemeSnapshot, String> {
    let home = std::env::var_os("HOME").ok_or("HOME is not set")?;
    let root = std::path::Path::new(&home).join(".config");
    let settings = read_optional_file(&root.join("arcoscope/gui-settings.json"))?;
    let overrides = read_optional_file(&root.join("zed/arcoscope-themes.json"))?;
    theme_snapshot(settings.as_deref(), overrides.as_deref())
}

fn apply_arcoscope_theme(snapshot: ThemeSnapshot, cx: &mut App) {
    if cx
        .try_global::<ArcoscopeTheme>()
        .and_then(|state| state.snapshot.as_ref())
        == Some(&snapshot)
    {
        return;
    }
    let name = snapshot
        .binding
        .as_ref()
        .and_then(|binding| binding.theme.as_ref())
        .map(|name| SharedString::from(name.clone()));
    if theme_settings::set_external_theme(name.clone(), cx) {
        crate::arcoscope_skin::set_bitmap_skin(snapshot.binding.as_ref(), cx);
    } else {
        // Keep the user's ordinary theme when a mapping names an uninstalled one.
        // Do not record this snapshot: a later registry load should retry it.
        if cx
            .try_global::<ArcoscopeTheme>()
            .and_then(|state| state.missing_theme.as_ref())
            != name.as_ref()
        {
            log::warn!(
                "Arcoscope theme {:?} is not installed; using the configured Zed theme",
                name
            );
        }
        theme_settings::set_external_theme(None, cx);
        crate::arcoscope_skin::set_bitmap_skin(None, cx);
        cx.set_global(ArcoscopeTheme {
            snapshot: None,
            missing_theme: name,
        });
        return;
    }
    cx.set_global(ArcoscopeTheme {
        snapshot: Some(snapshot),
        missing_theme: None,
    });
    cx.refresh_windows();
}

pub fn start_arcoscope_theme_watch(cx: &mut App) {
    cx.spawn(async move |cx| {
        let mut previous_error = None;
        loop {
            match cx
                .background_executor()
                .spawn(async { read_arcoscope_theme() })
                .await
            {
                Ok(snapshot) => {
                    previous_error = None;
                    cx.update(|cx| apply_arcoscope_theme(snapshot, cx));
                }
                Err(error) => {
                    if previous_error.as_ref() != Some(&error) {
                        log::error!("Arcoscope theme configuration: {error}");
                        previous_error = Some(error);
                    }
                }
            }
            cx.background_executor()
                .timer(std::time::Duration::from_millis(1500))
                .await;
        }
    })
    .detach();
}

#[cfg(test)]
mod theme_binding_tests {
    use super::*;

    #[test]
    fn maps_arbitrary_future_names_without_rust_changes() {
        let snapshot = theme_snapshot(
            Some(r#"{"barTheme":"future-theme"}"#),
            Some(r#"{"themes":{"future-theme":{"theme":"Future Zed"}}}"#),
        )
        .expect("valid mapping");
        assert_eq!(
            snapshot
                .binding
                .and_then(|binding| binding.theme)
                .as_deref(),
            Some("Future Zed")
        );
    }

    #[test]
    fn disabled_or_unmapped_settings_restore_the_configured_theme() {
        assert!(
            theme_snapshot(
                Some(r#"{"barTheme":"dreamweb"}"#),
                Some(r#"{"enabled":false}"#)
            )
            .expect("valid config")
            .binding
            .is_none()
        );
        assert!(
            theme_snapshot(Some(r#"{"barTheme":"unmapped"}"#), None)
                .expect("valid config")
                .binding
                .is_none()
        );
    }

    #[test]
    fn vector_themes_map_to_their_bundled_zed_themes() {
        for (bar_theme, zed_theme) in [("mist", "Mist"), ("gruvbox-dark", "Gruvbox Dark (Arcoscope)")]
        {
            let binding = theme_snapshot(Some(&format!(r#"{{"barTheme":"{bar_theme}"}}"#)), None)
                .expect("valid config")
                .binding
                .expect("bundled mapping");
            assert_eq!(binding.theme.as_deref(), Some(zed_theme));
            assert_eq!(binding.pixel_art, Some(false));
            assert!(binding.skin.is_some());
        }
    }

    #[test]
    fn invalid_configuration_does_not_silently_switch_theme() {
        assert!(theme_snapshot(Some("{"), None).is_err());
        assert!(theme_snapshot(None, Some("{")).is_err());
    }
}

fn mix(a: Hsla, b: Hsla, t: f32) -> Hsla {
    let a: Rgba = a.into();
    let b: Rgba = b.into();
    Rgba {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: 1.0,
    }
    .into()
}

/// `color` blended `amount` toward white.
pub fn arcoscope_lighten(color: Hsla, amount: f32) -> Hsla {
    mix(color, gpui::white(), amount)
}

/// `color` blended `amount` toward black.
pub fn arcoscope_darken(color: Hsla, amount: f32) -> Hsla {
    mix(color, gpui::black(), amount)
}

/// The current arcoscope page, or `None` when arcoscope has not said. Skinned
/// chrome picks its per-collection bitmap with it (`arcoscope_skin_surface_variant`).
pub fn arcoscope_page(cx: &App) -> Option<usize> {
    cx.try_global::<ArcoscopePage>().and_then(|page| page.0)
}

/// `arcoscope_page` for the editor's skinned chrome (its selected tab and the
/// workspace's bottom strip), on the same terms as `arcoscope_bar_background`:
/// `None` while a terminal column or another app holds focus, so the editor
/// then draws its neutral bitmaps.
pub fn arcoscope_bar_page(window: &Window, cx: &App) -> Option<usize> {
    editor_holds_arcoscope_focus(window, cx)
        .then(|| arcoscope_page(cx))
        .flatten()
}

/// The accent line along the top of the active Amiga tab: the current arcoscope
/// page's colour, brightened, so the tab you are in carries the colour of the
/// bar's current cell. Blue when arcoscope has not said.
pub fn arcoscope_amiga_accent(cx: &App) -> Hsla {
    match cx
        .try_global::<ArcoscopePage>()
        .and_then(|page| page.0)
        .and_then(arcoscope_page_accent)
    {
        Some(accent) => arcoscope_lighten(rgb(accent).into(), 0.25),
        None => rgb(0x5aa0e6).into(),
    }
}

/// Amiga tab title colours, the Ghostty fork's: a light beige on the active
/// tab, a readable muted beige on the rest.
pub fn arcoscope_amiga_text(selected: bool) -> Hsla {
    rgb(if selected { 0xe0d0ae } else { 0xbdae93 }).into()
}

/// The second line of an Amiga tab (the Ghostty fork's worktree line): a dimmer
/// beige under the title.
pub fn arcoscope_amiga_subtext(selected: bool) -> Hsla {
    rgb(if selected { 0xa89984 } else { 0x7c6f64 }).into()
}

thread_local! {
    static WINDOW_MOVE_ARMED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Makes an empty part of the window's top row drag the window: Sinter under
/// arcoscope has no title bar, so the terminal column's tab strip and the
/// editor's tab bar stand in for it. Only a press on the element itself arms
/// the move, so a drag that starts on a tab and slides over it does not move
/// the window.
pub fn arcoscope_window_drag_area<E: InteractiveElement>(element: E) -> E {
    element
        .on_mouse_down(MouseButton::Left, |_, _, _| WINDOW_MOVE_ARMED.set(true))
        .on_mouse_up(MouseButton::Left, |_, _, _| WINDOW_MOVE_ARMED.set(false))
        .on_mouse_move(|event, window, _| {
            if WINDOW_MOVE_ARMED.replace(false) && event.pressed_button == Some(MouseButton::Left) {
                window.start_window_move();
            }
        })
}
