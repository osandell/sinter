//! arcoscope's hint badges, drawn by the window they label: letters over the
//! terminal and editor tabs (hold 3), the dock's rows (hold §) and words in
//! the terminal's output (hold + or ´). The keys stay arcoscope's; it says
//! over the control socket which badges to show and which one was picked.
//!
//! Drawing them here rather than in a window of arcoscope's own over this one
//! needs no screen coordinates, so they land right on any display and scale,
//! and on the window that has the keyboard rather than one arcoscope guessed.
//! The tab and dock rows they label report their places here as they render,
//! in window-local points, so a row that moves (a folder opened under §)
//! takes its badge along on the next frame.

use collections::HashMap;
use gpui::{
    AnyElement, AnyWindowHandle, App, EntityId, Global, IntoElement, ParentElement, Rgba,
    SharedString, Styled, Window, WindowId, deferred, div, px, rgb,
};
use serde::Deserialize;
use settings::Settings as _;

/// A badge, in its window's points.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Badge {
    pub label: char,
    /// The badge's left edge, or its right edge when it `hangs`.
    pub x: f32,
    /// Its vertical centre.
    pub y: f32,
    /// Hangs off the left of `x`, so it does not cover the word it labels.
    #[serde(default)]
    pub hangs: bool,
    #[serde(default)]
    pub height: Option<f32>,
    /// What it labels, outlined: one rect (x, y, w, h) per row.
    #[serde(default)]
    pub outline: Vec<[f32; 4]>,
}

/// One kind of badges, for [`clear`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Tabs,
    Sidebar,
    Words,
}

#[derive(Default)]
struct WindowHints {
    handle: Option<AnyWindowHandle>,
    /// Each pane's tab row: its centre and every tab's left edge.
    editor_tabs: HashMap<EntityId, (f32, Vec<f32>)>,
    /// The dock's rows, `(target index, x, y)`, empty while the panel is not
    /// the one on screen.
    project_rows: Vec<(usize, f32, f32)>,
    git_rows: Vec<(usize, f32, f32)>,
    terminal_tabs: Vec<Badge>,
    editor_labels: Option<Vec<char>>,
    sidebar: Option<(Vec<char>, usize)>,
    words: Vec<Badge>,
}

impl WindowHints {
    fn showing(&self) -> bool {
        !self.terminal_tabs.is_empty()
            || self.editor_labels.is_some()
            || self.sidebar.is_some()
            || !self.words.is_empty()
    }
}

#[derive(Default)]
struct ArcoscopeHints(HashMap<WindowId, WindowHints>);

impl Global for ArcoscopeHints {}

fn entry(handle: AnyWindowHandle, cx: &mut App) -> &mut WindowHints {
    let hints = cx
        .default_global::<ArcoscopeHints>()
        .0
        .entry(handle.window_id())
        .or_default();
    hints.handle = Some(handle);
    hints
}

/// Draws `handle` again: from inside its own render the window is leased, so
/// the refresh waits for the frame to end.
fn refresh(handle: AnyWindowHandle, cx: &mut App) {
    cx.defer(move |cx| {
        handle.update(cx, |_, window, _| window.refresh()).ok();
    });
}

/// A pane's tab row as it was laid out (`Pane::report_tab_geometry`).
pub fn set_editor_tabs(window: &Window, pane: EntityId, center: f32, xs: Vec<f32>, cx: &mut App) {
    let handle = window.window_handle();
    let hints = entry(handle, cx);
    let row = (center, xs);
    if hints.editor_tabs.get(&pane) == Some(&row) {
        return;
    }
    hints.editor_tabs.insert(pane, row);
    if hints.editor_labels.is_some() {
        refresh(handle, cx);
    }
}

/// The project (`git` false) or git panel's rows as they were laid out.
pub fn set_sidebar_rows(window: &Window, git: bool, rows: Vec<(usize, f32, f32)>, cx: &mut App) {
    let handle = window.window_handle();
    let hints = entry(handle, cx);
    let slot = if git {
        &mut hints.git_rows
    } else {
        &mut hints.project_rows
    };
    if *slot == rows {
        return;
    }
    *slot = rows;
    if hints.sidebar.is_some() {
        refresh(handle, cx);
    }
}

/// Whether § labels the git panel in `handle` (it is the dock's panel on
/// screen) rather than the project panel. The git panel is one page.
pub fn sidebar_is_git(handle: AnyWindowHandle, cx: &App) -> bool {
    cx.try_global::<ArcoscopeHints>()
        .and_then(|hints| hints.0.get(&handle.window_id()))
        .is_some_and(|hints| !hints.git_rows.is_empty())
}

/// Hold 3: the terminal's tab badges (placed by its columns) and the editor's
/// tabs labelled with `editor_labels`, in `handle`.
pub fn show_tabs(
    handle: AnyWindowHandle,
    terminal: Vec<Badge>,
    editor_labels: Vec<char>,
    cx: &mut App,
) {
    clear_except(Kind::Tabs, handle, cx);
    let hints = entry(handle, cx);
    hints.terminal_tabs = terminal;
    hints.editor_labels = Some(editor_labels);
    refresh(handle, cx);
}

/// Hold §: the dock's rows of `page`, labelled with `labels`, in `handle`.
pub fn show_sidebar(handle: AnyWindowHandle, labels: Vec<char>, page: usize, cx: &mut App) {
    clear_except(Kind::Sidebar, handle, cx);
    entry(handle, cx).sidebar = Some((labels, page));
    refresh(handle, cx);
}

/// Hold + or ´: words in the terminal's output, placed by arcoscope.
pub fn show_words(handle: AnyWindowHandle, badges: Vec<Badge>, cx: &mut App) {
    clear_except(Kind::Words, handle, cx);
    entry(handle, cx).words = badges;
    refresh(handle, cx);
}

/// The window `kind` is shown in.
pub fn shown_in(kind: Kind, cx: &App) -> Option<AnyWindowHandle> {
    let hints = cx.try_global::<ArcoscopeHints>()?;
    hints.0.values().find_map(|hints| {
        let shown = match kind {
            Kind::Tabs => !hints.terminal_tabs.is_empty() || hints.editor_labels.is_some(),
            Kind::Sidebar => hints.sidebar.is_some(),
            Kind::Words => !hints.words.is_empty(),
        };
        shown.then_some(hints.handle).flatten()
    })
}

/// The target index of § badge 0: its page times the labels on a page (the
/// git panel is one page).
pub fn sidebar_base(cx: &App) -> Option<usize> {
    let hints = cx.try_global::<ArcoscopeHints>()?;
    hints.0.values().find_map(|hints| {
        let (labels, page) = hints.sidebar.as_ref()?;
        Some(if hints.git_rows.is_empty() {
            page * labels.len()
        } else {
            0
        })
    })
}

/// What § does to the dock's panels, which this crate cannot reach: set by
/// the app.
#[derive(Clone, Copy)]
pub struct SidebarActions {
    /// Opens target `index` of the git (true) or project panel in the window.
    pub open: fn(AnyWindowHandle, bool, usize, &mut App),
    /// Scrolls the project panel to target `index`, `"top"` or `"bottom"`.
    pub scroll: fn(AnyWindowHandle, usize, &'static str, &mut App),
}

impl Global for SidebarActions {}

pub fn set_sidebar_actions(actions: SidebarActions, cx: &mut App) {
    cx.set_global(actions);
}

pub fn sidebar_actions(cx: &App) -> Option<SidebarActions> {
    cx.try_global::<SidebarActions>().copied()
}

/// Takes `kind` away (all kinds for None), in every window.
pub fn clear(kind: Option<Kind>, cx: &mut App) {
    clear_where(kind, None, cx);
}

/// A kind goes up in one window at a time: the same kind still up in another
/// window goes.
fn clear_except(kind: Kind, keep: AnyWindowHandle, cx: &mut App) {
    clear_where(Some(kind), Some(keep.window_id()), cx);
}

fn clear_where(kind: Option<Kind>, keep: Option<WindowId>, cx: &mut App) {
    if !cx.has_global::<ArcoscopeHints>() {
        return;
    }
    let mut changed = Vec::new();
    for (id, hints) in cx.global_mut::<ArcoscopeHints>().0.iter_mut() {
        if Some(*id) == keep || !hints.showing() {
            continue;
        }
        if matches!(kind, None | Some(Kind::Tabs)) {
            hints.terminal_tabs.clear();
            hints.editor_labels = None;
        }
        if matches!(kind, None | Some(Kind::Sidebar)) {
            hints.sidebar = None;
        }
        if matches!(kind, None | Some(Kind::Words)) {
            hints.words.clear();
        }
        changed.extend(hints.handle);
    }
    for handle in changed {
        refresh(handle, cx);
    }
}

/// The finger cluster of a hint's key, which colours its badge (the same as
/// arcoscope's `hints::cluster`).
fn fill(label: char) -> Rgba {
    rgb(match label {
        'm' | 'n' | 'e' | 'i' | 'o' | '\'' | 'a' | 'r' | 's' | 't' | 'g' => 0x429a4e,
        'l' | 'u' | 'y' | '-' | 'j' | '/' | 'q' | 'w' | 'f' | 'p' | 'b' => 0xb03a6e,
        'k' | 'h' | ',' | '.' | '\u{e5}' | 'z' | 'x' | 'c' | 'd' | 'v' => 0x3d7ad6,
        '9' | '\u{e4}' | '+' | '1' | '\u{f6}' => 0xfb4934,
        _ => 0x665c54,
    })
}

/// A tab badge sits this far in from the tab's left edge.
const LEAD_IN: f32 = 2.;
/// A badge's height when it is not lined up with an outline.
const BADGE_HEIGHT: f32 = 20.;
const BADGE_RADIUS: f32 = 2.;
const TEXT_SIZE: f32 = 13.;

/// The badges up in `window`, over everything in it; `active_pane` is the pane
/// whose tabs hold 3 labels. Clicks go through.
pub fn render_overlay(window: &Window, active_pane: EntityId, cx: &App) -> Option<AnyElement> {
    let hints = cx
        .try_global::<ArcoscopeHints>()?
        .0
        .get(&window.window_handle().window_id())?;
    if !hints.showing() {
        return None;
    }
    let mut badges: Vec<Badge> = hints.terminal_tabs.clone();
    if let Some(labels) = &hints.editor_labels
        && let Some((center, xs)) = hints.editor_tabs.get(&active_pane)
    {
        badges.extend(labels.iter().zip(xs).map(|(&label, &x)| Badge {
            label,
            x,
            y: *center,
            hangs: false,
            height: None,
            outline: Vec::new(),
        }));
    }
    if let Some((labels, page)) = &hints.sidebar {
        let (rows, page) = if hints.git_rows.is_empty() {
            (&hints.project_rows, *page)
        } else {
            (&hints.git_rows, 0)
        };
        let base = page * labels.len();
        for (slot, &label) in labels.iter().enumerate() {
            if let Some(&(_, x, y)) = rows.iter().find(|(index, _, _)| *index == base + slot) {
                badges.push(Badge {
                    label,
                    x,
                    y,
                    hangs: false,
                    height: None,
                    outline: Vec::new(),
                });
            }
        }
    }
    badges.extend(hints.words.iter().cloned());

    let width = f32::from(window.bounds().size.width);
    let family: SharedString = theme_settings::ThemeSettings::get_global(cx)
        .buffer_font
        .family
        .clone();
    let mut layer = div().absolute().inset_0().size_full();
    for badge in badges {
        let color = fill(badge.label);
        for rect in &badge.outline {
            layer = layer.child(
                div()
                    .absolute()
                    .left(px(rect[0]))
                    .top(px(rect[1]))
                    .w(px(rect[2]))
                    .h(px(rect[3]))
                    .border_1()
                    .border_color(color)
                    .rounded(px(BADGE_RADIUS)),
            );
        }
        let height = badge.height.unwrap_or(BADGE_HEIGHT);
        let label = div()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .h(px(height))
            .px(px(2.))
            .rounded(px(BADGE_RADIUS))
            .bg(color)
            .font_family(family.clone())
            .text_size(px(TEXT_SIZE))
            .text_color(rgb(0xffffff))
            .child(SharedString::from(badge.label.to_string()));
        let row = div()
            .absolute()
            .top(px(badge.y - height / 2.))
            .h(px(height))
            .flex()
            .items_center();
        let row = if badge.hangs {
            row.right(px(width - badge.x)).justify_end()
        } else {
            row.left(px(badge.x + LEAD_IN))
        };
        layer = layer.child(row.child(label));
    }
    Some(deferred(layer).with_priority(3).into_any_element())
}
