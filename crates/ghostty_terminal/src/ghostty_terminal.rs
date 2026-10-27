//! A terminal backed by Ghostty, in a column beside the editor.
//!
//! macOS embeds the whole of libghostty (`terminal_mac.rs`, Ghostty's own Metal
//! renderer). Linux embeds libghostty-vt, Ghostty's terminal core, and draws
//! its cells with GPUI (`terminal_linux.rs`). Each platform file provides the
//! same `GhosttyTerminal` and the same `runtime`, `sheets` and `ffi` modules,
//! so the column, tabs, sessions and arcoscope's socket are shared.

#![cfg(any(target_os = "macos", target_os = "linux"))]

mod blocked_note;
mod claude_status;
mod close_confirm;
mod columns;
mod command_palette;
mod graphics;
pub mod lf_view;
mod remote_session;
mod session_activity;
mod tab_sessions;
mod terminal_column;
mod arcoscope;
mod worktree_picker;

#[cfg(target_os = "macos")]
mod input_view;
#[cfg(target_os = "macos")]
mod runtime;
#[cfg(target_os = "macos")]
mod sheets;
#[cfg(target_os = "macos")]
use ghostty_embed as ffi;

#[cfg(target_os = "linux")]
#[path = "linux_runtime.rs"]
mod runtime;
#[cfg(target_os = "linux")]
#[path = "linux_sheets.rs"]
mod sheets;
#[cfg(target_os = "linux")]
#[path = "linux_ffi.rs"]
mod ffi;

pub use columns::{ColumnSide, TerminalColumns};
pub use terminal_column::{
    ClaudeState, PickWorktree, TerminalColumn, TerminalColumnEvent, TerminalTab, worktree_split,
};

use gpui::{App, AppContext as _, Context, Entity, Focusable as _, Window, actions, px};
use std::path::PathBuf;
use gpui::WeakEntity;
use workspace::Workspace;
use util::ResultExt as _;

#[cfg(target_os = "macos")]
#[path = "terminal_mac.rs"]
mod terminal;
#[cfg(target_os = "linux")]
#[path = "terminal_linux.rs"]
mod terminal;
pub use terminal::*;

actions!(
    ghostty_terminal,
    [
        /// Opens a new Ghostty terminal in the active pane.
        NewGhosttyTerminal,
        /// Fullscreen for the side that has the keyboard (terminal or editor).
        ToggleFullscreen,
        /// Moves the keyboard to the terminal column.
        FocusTerminal,
        /// Moves the keyboard to the editor.
        FocusEditor,
        /// Opens the Ghostty config file in the editor.
        OpenConfig,
        /// Reloads the Ghostty config files.
        ReloadConfig,
        /// Opens a shell tab in the terminal column.
        NewShellTab,
        /// Opens a tab in the terminal column running the default agent
        /// (`agent.command` in arcoscope's settings.json, else `claude`).
        NewAgentTab,
        /// Switches the right side of the window between the editor and a
        /// terminal column of its own.
        ToggleRightSideTerminal,
    ]
);

fn column_of(workspace: &Workspace) -> Option<Entity<TerminalColumn>> {
    workspace
        .leading_column()?
        .clone()
        .downcast::<TerminalColumn>()
        .ok()
}

/// The terminal column standing where the editor is, while the right side is
/// in terminal mode.
fn trailing_column_of(workspace: &Workspace) -> Option<Entity<TerminalColumn>> {
    workspace
        .trailing_column()?
        .clone()
        .downcast::<TerminalColumn>()
        .ok()
}

/// The right side shows a terminal column instead of the editor.
pub fn right_side_terminal(cx: &App) -> bool {
    TerminalColumns::right_side_terminal(cx)
}

/// Puts the right side of the window in terminal mode, or back to the editor.
/// The work is deferred: this is called from a click inside a column and from
/// arcoscope's socket, and the sync it needs updates both.
pub fn set_right_side_terminal(on: bool, cx: &mut App) {
    if TerminalColumns::right_side_terminal(cx) == on {
        return;
    }
    TerminalColumns::set_right_side_terminal(on, cx);
    let Some(window) = workspace::unified_window_handle(cx) else {
        return;
    };
    cx.defer(move |cx| {
        window
            .update(cx, |multi_workspace, window, cx| {
                columns::sync(multi_workspace, window, cx);
                let workspace = multi_workspace.workspace().clone();
                // The keyboard goes to the right side either way: it is the
                // terminal column now, or the editor again.
                workspace.update(cx, |workspace, cx| {
                    focus_editor(workspace, window, cx);
                });
            })
            .log_err();
    });
}

/// Moves the keyboard to the right side of the window: the editor, or the
/// terminal column standing in its place. The mode itself is not changed;
/// only `set_right_side_terminal` does that.
pub fn focus_editor(workspace: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
    if trailing_column_of(workspace).is_some() {
        focus_trailing_terminal(workspace, window, cx);
        return;
    }
    if let Some(column) = column_of(workspace) {
        let layout = column.update(cx, |column, cx| column.prepare_side(false, cx));
        workspace.set_leading_column_layout(layout, cx);
    }
    let focus_handle = match workspace.active_item(cx) {
        Some(item) => item.item_focus_handle(cx),
        None => workspace.active_pane().focus_handle(cx),
    };
    window.focus(&focus_handle, cx);
}

/// Shows `target` in the unified window without moving the keyboard out of the
/// terminal. `MultiWorkspace::activate` focuses the new workspace's pane, which
/// is right for a worktree switch and wrong when the editor only follows the
/// terminal tab's worktree: p+q/w to a tab in another worktree then left the
/// keyboard, and arcoscope's virtual keys, on the editor.
pub fn show_workspace_keeping_terminal_focus(
    multi_workspace: &mut workspace::MultiWorkspace,
    target: Entity<Workspace>,
    window: &mut Window,
    cx: &mut Context<workspace::MultiWorkspace>,
) {
    if multi_workspace.workspace() == &target {
        return;
    }
    let terminal = ui::arcoscope_terminal_focused(window, cx)
        .then(|| TerminalColumns::current(cx))
        .flatten();
    multi_workspace.activate(target, None, window, cx);
    if let Some(column) = terminal {
        window.focus(&column.focus_handle(cx), cx);
    }
}

/// arcoscope's terminal width (800 pt, 650 at a 50 % width factor) for every
/// terminal column.
pub fn set_terminal_width(width: f32, cx: &mut App) {
    for column in TerminalColumns::all(cx) {
        column.update(cx, |column, cx| column.set_column_width(px(width), cx));
    }
}

/// arcoscope's q+f: fullscreen for the side that has the keyboard.
pub fn toggle_fullscreen(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    if let Some(column) = column_of(workspace) {
        let layout = column.update(cx, |column, cx| column.toggle_fullscreen(window, cx));
        workspace.set_leading_column_layout(layout, cx);
    }
}

/// arcoscope's fullscreen set rather than toggled (`state=on|off`), so a
/// repeated request keeps it. With `terminal_side` the keyboard goes to that
/// half, which then fills the window.
pub fn set_fullscreen(
    workspace: &mut Workspace,
    on: bool,
    terminal_side: Option<bool>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let Some(column) = column_of(workspace) else {
        return;
    };
    column.update(cx, |column, cx| column.set_fullscreen(on, window, cx));
    match terminal_side {
        Some(true) => focus_terminal(workspace, window, cx),
        Some(false) => focus_editor(workspace, window, cx),
        None => {
            let layout = column.read(cx).layout();
            workspace.set_leading_column_layout(layout, cx);
        }
    }
}

/// Moves the keyboard to the terminal column on the left. In fullscreen the
/// column is laid out first, since a hidden column cannot take focus.
pub fn focus_terminal(workspace: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
    if let Some(column) = column_of(workspace) {
        let layout = column.update(cx, |column, cx| column.prepare_side(true, cx));
        workspace.set_leading_column_layout(layout, cx);
        window.focus(&column.focus_handle(cx), cx);
    }
}

/// Moves the keyboard to the terminal column it was in last: the right side
/// when that is a terminal and was where the keyboard came from, the left one
/// otherwise. arcoscope's terminal key and its dock entry land here.
pub fn focus_last_terminal(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    if TerminalColumns::last_focused_is_trailing(cx) && trailing_column_of(workspace).is_some() {
        focus_trailing_terminal(workspace, window, cx);
        return;
    }
    focus_terminal(workspace, window, cx);
}

/// Moves the keyboard to the terminal column on the right. The column on the
/// left gives up its claim first, so a fullscreen window shows this side.
fn focus_trailing_terminal(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let Some(trailing) = trailing_column_of(workspace) else {
        return;
    };
    if let Some(leading) = column_of(workspace) {
        let layout = leading.update(cx, |column, _| column.release_keyboard());
        workspace.set_leading_column_layout(layout, cx);
    }
    trailing.update(cx, |column, cx| column.prepare_trailing_side(cx));
    window.focus(&trailing.focus_handle(cx), cx);
}

/// The terminal column a new tab belongs in: the one with the keyboard, else
/// the left one, which is shown and focused first.
fn focused_column(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Option<Entity<TerminalColumn>> {
    let trailing = trailing_column_of(workspace);
    let in_trailing = trailing.as_ref().is_some_and(|column| {
        column
            .read(cx)
            .focus_handle_ref()
            .contains_focused(window, cx)
    });
    if in_trailing {
        return trailing;
    }
    focus_terminal(workspace, window, cx);
    if trailing.is_some() && TerminalColumns::last_focused_is_trailing(cx) {
        return trailing;
    }
    column_of(workspace)
}

pub use arcoscope::owns_scopes;

pub fn init(cx: &mut App) {
    // Zed and the terminal are one app with one window holding every
    // workspace.
    cx.set_global(workspace::UnifiedWindow);
    arcoscope::init(cx);

    cx.observe_new(|workspace: &mut Workspace, window, cx| {
        let Some(window) = window else {
            return;
        };
        if workspace.project().read(cx).is_local() {
            let handle = cx.entity().downgrade();
            let project = workspace.project().clone();
            let column = cx.new(|cx| {
                TerminalColumn::new(
                    handle.clone(),
                    ColumnSide::Leading,
                    Some(project),
                    window,
                    cx,
                )
            });
            TerminalColumns::register(handle, ColumnSide::Leading, column.clone(), cx);
            arcoscope::watch_column(&column, cx);
            workspace.set_leading_column(Some(column.into()), cx);
            cx.on_release(|_, cx| {
                // `on_release` has no handle to the released entity; drop every
                // registration whose workspace is gone.
                if let Some(columns) = cx.try_global::<TerminalColumns>() {
                    let gone: Vec<_> = columns
                        .owners()
                        .filter(|owner| owner.upgrade().is_none())
                        .collect();
                    for owner in gone {
                        TerminalColumns::unregister(&owner, cx);
                    }
                }
            })
            .detach();
        }
        workspace.register_action(|workspace, _: &ToggleFullscreen, window, cx| {
            toggle_fullscreen(workspace, window, cx);
        });
        workspace.register_action(|workspace, _: &FocusTerminal, window, cx| {
            focus_terminal(workspace, window, cx);
        });
        workspace.register_action(|workspace, _: &FocusEditor, window, cx| {
            focus_editor(workspace, window, cx);
        });
        workspace.register_action(|_, _: &ToggleRightSideTerminal, _, cx| {
            set_right_side_terminal(!TerminalColumns::right_side_terminal(cx), cx);
        });
        // From the command palette, with the keyboard anywhere: the column is
        // shown and gets the new tab (and the keyboard).
        workspace.register_action(|workspace, _: &NewShellTab, window, cx| {
            if let Some(column) = focused_column(workspace, window, cx) {
                column.update(cx, |column, cx| column.new_tab_following_worktree(window, cx));
            }
        });
        workspace.register_action(|workspace, _: &NewAgentTab, window, cx| {
            if let Some(column) = focused_column(workspace, window, cx) {
                column.update(cx, |column, cx| column.new_agent_tab(window, cx));
            }
        });
        workspace.register_action(|_, _: &OpenConfig, _, cx| runtime::open_config(cx));
        workspace.register_action(|_, _: &ReloadConfig, _, _| runtime::reload_config());
    })
    .detach();

    // Pair the current terminal with whichever editor the window shows, and
    // let only that column draw.
    cx.observe_new(|_: &mut workspace::MultiWorkspace, window, cx| {
        let Some(window) = window else {
            return;
        };
        cx.subscribe_in(
            &cx.entity(),
            window,
            |multi_workspace, _, event, window, cx| {
                if matches!(
                    event,
                    workspace::MultiWorkspaceEvent::ActiveWorkspaceChanged { .. }
                        | workspace::MultiWorkspaceEvent::WorkspaceAdded(_)
                        | workspace::MultiWorkspaceEvent::WorkspaceRemoved(_)
                ) {
                    columns::sync(multi_workspace, window, cx);
                }
            },
        )
        .detach();
    })
    .detach();

    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(|workspace, _: &NewGhosttyTerminal, window, cx| {
            let working_directory = workspace
                .project()
                .read(cx)
                .visible_worktrees(cx)
                .next()
                .map(|worktree| worktree.read(cx).abs_path().to_path_buf());
            let options = TerminalOptions {
                working_directory,
                ..Default::default()
            };
            match GhosttyTerminal::open(options, window, cx) {
                Ok(terminal) => {
                    workspace.add_item_to_active_pane(Box::new(terminal), None, true, window, cx);
                }
                Err(error) => {
                    log::error!("failed to open a Ghostty terminal: {error:#}");
                    workspace.show_error(&error, cx);
                }
            }
        });
    })
    .detach();
}

pub enum GhosttyTerminalEvent {
    TitleChanged,
    PwdChanged,
    CloseRequested {
        process_alive: bool,
    },
    Focused,
    NewSplit(ffi::ghostty_action_split_direction_e),
    NewTab,
    CloseTab(ffi::ghostty_action_close_tab_mode_e),
    GotoTab(i32),
    GotoSplit(ffi::ghostty_action_goto_split_e),
    ResizeSplit {
        direction: ffi::ghostty_action_resize_split_direction_e,
        amount: u16,
    },
    EqualizeSplits,
    ToggleSplitZoom,
    ToggleCommandPalette,
}

/// How to start a new terminal.
#[derive(Default)]
pub struct TerminalOptions {
    pub working_directory: Option<PathBuf>,
    /// Run instead of the configured shell; the terminal closes when it exits.
    pub command: Option<String>,
    /// Typed into the shell once it starts, e.g. `claude --resume ...\n`.
    pub initial_input: Option<String>,
    /// Inherit font size, working directory etc. from this terminal, the way a
    /// new Ghostty tab or split does.
    pub inherit_from: Option<(WeakEntity<GhosttyTerminal>, InheritContext)>,
}

#[derive(Clone, Copy)]
pub enum InheritContext {
    Tab,
    Split,
}

