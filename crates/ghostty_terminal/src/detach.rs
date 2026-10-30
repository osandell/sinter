//! A scope broken out of the unified window into a window of its own, which
//! arcoscope puts on another display.
//!
//! The scope's workspace stays in the unified window (its home), hidden there,
//! so its editor tabs and its place in the session are kept. The window of its
//! own gets a new workspace on the same project, and the scope's terminal
//! columns move to it with their terminals running: no shell or Claude session
//! restarts. Attaching moves the columns back home and closes the window.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, anyhow};
use gpui::{App, AppContext as _, Entity, Global, TaskExt as _, WeakEntity, WindowHandle};
use util::ResultExt as _;
use workspace::{DetachedWindows, MultiWorkspace, Workspace};

use crate::columns::{self, TerminalColumns};
use crate::{ColumnSide, TerminalColumn};

struct Detached {
    path: PathBuf,
    home: WeakEntity<Workspace>,
    window: WindowHandle<MultiWorkspace>,
}

#[derive(Default)]
struct DetachedScopes {
    scopes: Vec<Detached>,
    /// The columns the next workspace made takes over instead of opening a
    /// column of its own (set just before the broken-out window is made).
    adopting: Option<Vec<(ColumnSide, Entity<TerminalColumn>)>>,
}

impl Global for DetachedScopes {}

/// The columns a workspace being made right now is to take over.
pub(crate) fn take_adoption(cx: &mut App) -> Option<Vec<(ColumnSide, Entity<TerminalColumn>)>> {
    cx.default_global::<DetachedScopes>().adopting.take()
}

/// The window the scope of the worktree at `path` was broken out into.
pub fn window_of(path: &Path, cx: &App) -> Option<WindowHandle<MultiWorkspace>> {
    cx.try_global::<DetachedScopes>()?
        .scopes
        .iter()
        .find(|detached| detached.path == path)
        .map(|detached| detached.window)
}

/// Whether `workspace` is the home of a scope broken out: hidden in the
/// unified window, never shown there.
pub fn is_home(workspace: &Entity<Workspace>, cx: &App) -> bool {
    cx.try_global::<DetachedScopes>().is_some_and(|detached| {
        detached
            .scopes
            .iter()
            .any(|scope| scope.home.upgrade().as_ref() == Some(workspace))
    })
}

/// Whether `window` is the window of a scope broken out.
pub fn is_detached_window(window: WindowHandle<MultiWorkspace>, cx: &App) -> bool {
    workspace::is_detached_window(window.window_id(), cx)
}

/// Every scope broken out, with its window.
pub fn all(cx: &App) -> Vec<(PathBuf, WindowHandle<MultiWorkspace>)> {
    cx.try_global::<DetachedScopes>()
        .map(|detached| {
            detached
                .scopes
                .iter()
                .map(|scope| (scope.path.clone(), scope.window))
                .collect()
        })
        .unwrap_or_default()
}

/// The id arcoscope frames a window with: the macOS window number. Linux
/// finds the window by its title (the worktree's path) and gets the GPUI id.
pub fn native_id(window: WindowHandle<MultiWorkspace>, cx: &mut App) -> Option<i64> {
    window
        .update(cx, |_, window, _| native_window_id(window))
        .ok()
        .flatten()
}

#[cfg(target_os = "macos")]
fn native_window_id(window: &gpui::Window) -> Option<i64> {
    use cocoa::base::id;
    use objc::{msg_send, sel, sel_impl};
    let ns_window = crate::gpui_native_window(window).ok()?;
    let number: isize = unsafe { msg_send![ns_window as id, windowNumber] };
    Some(number as i64)
}

#[cfg(not(target_os = "macos"))]
fn native_window_id(window: &gpui::Window) -> Option<i64> {
    Some(window.window_handle().window_id().as_u64() as i64)
}

/// Breaks the scope of the worktree at `path` out into a window of its own,
/// or finds the one it is in.
pub fn detach(path: &Path, cx: &mut App) -> Result<WindowHandle<MultiWorkspace>> {
    if let Some(window) = window_of(path, cx) {
        return Ok(window);
    }
    let column = TerminalColumns::column_for_path(path, cx).ok_or_else(|| anyhow!("no-window"))?;
    let home_handle = column.read(cx).workspace().clone();
    let home = home_handle.upgrade().ok_or_else(|| anyhow!("no-window"))?;
    let unified = workspace::unified_window_handle(cx).ok_or_else(|| anyhow!("no-unified-window"))?;
    let others = unified
        .read(cx)
        .map(|multi_workspace| {
            multi_workspace
                .workspaces()
                .filter(|workspace| *workspace != &home && !is_home(workspace, cx))
                .count()
        })
        .unwrap_or(0);
    // The unified window always shows some scope.
    if others == 0 {
        return Err(anyhow!("last-scope"));
    }

    let mut adopted = vec![(ColumnSide::Leading, column.clone())];
    if let Some(trailing) = TerminalColumns::trailing_for_workspace(&home_handle, cx) {
        adopted.push((ColumnSide::Trailing, trailing));
    }
    let (project, app_state) = {
        let home = home.read(cx);
        (home.project().clone(), home.app_state().clone())
    };
    let open_paths: Vec<_> = home
        .read(cx)
        .items(cx)
        .filter_map(|item| item.project_path(cx))
        .collect();
    let active_path = home
        .read(cx)
        .active_item(cx)
        .and_then(|item| item.project_path(cx));

    cx.default_global::<DetachedScopes>().adopting = Some(adopted);
    let options = (app_state.build_window_options)(None, cx);
    let opened = cx.open_window(options, {
        let app_state = app_state.clone();
        move |window, cx| {
            let workspace = cx.new(|cx| Workspace::new(None, project, app_state, window, cx));
            cx.new(|cx| MultiWorkspace::new(workspace, window, cx))
        }
    });
    // Taken by the new workspace; whatever is left (the window failed) goes.
    cx.default_global::<DetachedScopes>().adopting = None;
    let window = opened.context("could not open a window for the scope")?;

    cx.default_global::<DetachedWindows>()
        .0
        .insert(window.window_id());
    cx.default_global::<DetachedScopes>().scopes.push(Detached {
        path: path.to_path_buf(),
        home: home_handle,
        window,
    });

    // The unified window goes on with another scope when this one was on
    // screen there.
    unified
        .update(cx, |multi_workspace, window, cx| {
            if multi_workspace.workspace() == &home {
                let next = multi_workspace
                    .workspaces()
                    .find(|workspace| *workspace != &home && !is_home(workspace, cx))
                    .cloned();
                if let Some(next) = next {
                    multi_workspace.activate(next, None, window, cx);
                }
            }
            home.update(cx, |home, cx| {
                home.set_leading_column(None, cx);
                home.set_trailing_column(None, cx);
            });
        })
        .log_err();

    // The editor tabs of the home, in the window of its own; the window's
    // close button attaches the scope again rather than closing its terminals.
    let path = path.to_path_buf();
    window
        .update(cx, |multi_workspace, window, cx| {
            let workspace = multi_workspace.workspace().clone();
            workspace.update(cx, |workspace, cx| {
                for project_path in open_paths {
                    let focus = Some(&project_path) == active_path.as_ref();
                    workspace
                        .open_path(project_path, None, focus, window, cx)
                        .detach_and_log_err(cx);
                }
            });
            window.on_window_should_close(cx, move |_, cx| {
                if window_of(&path, cx).is_none() {
                    return true;
                }
                let path = path.clone();
                cx.defer(move |cx| {
                    attach(&path, cx).log_err();
                });
                false
            });
        })
        .log_err();
    columns::sync_all(cx);
    Ok(window)
}

/// Puts the scope of the worktree at `path` back into the unified window and
/// closes the window it had.
pub fn attach(path: &Path, cx: &mut App) -> Result<()> {
    let Some(index) = cx
        .try_global::<DetachedScopes>()
        .and_then(|detached| detached.scopes.iter().position(|scope| scope.path == path))
    else {
        return Ok(());
    };
    let detached = cx.default_global::<DetachedScopes>().scopes.remove(index);
    cx.default_global::<DetachedWindows>()
        .0
        .remove(&detached.window.window_id());
    let home = detached.home.upgrade().ok_or_else(|| anyhow!("home-gone"))?;
    let unified = workspace::unified_window_handle(cx).ok_or_else(|| anyhow!("no-unified-window"))?;

    let away = detached
        .window
        .read(cx)
        .map(|multi_workspace| multi_workspace.workspace().clone())
        .ok();
    let active_path = away
        .as_ref()
        .and_then(|workspace| workspace.read(cx).active_item(cx))
        .and_then(|item| item.project_path(cx));
    let columns: Vec<Entity<TerminalColumn>> = away
        .map(|workspace| {
            let owner = workspace.downgrade();
            [
                TerminalColumns::column_for_workspace(&owner, cx),
                TerminalColumns::trailing_for_workspace(&owner, cx),
            ]
            .into_iter()
            .flatten()
            .collect()
        })
        .unwrap_or_default();

    unified
        .update(cx, |_, window, cx| {
            for column in &columns {
                TerminalColumns::reown(column, detached.home.clone(), cx);
                column.update(cx, |column, cx| column.rehome(detached.home.clone(), window, cx));
            }
            if let Some(project_path) = active_path {
                home.update(cx, |home, cx| {
                    home.open_path(project_path, None, false, window, cx)
                        .detach_and_log_err(cx);
                });
            }
        })
        .log_err();
    detached
        .window
        .update(cx, |_, window, _| window.remove_window())
        .log_err();
    columns::sync_all(cx);
    Ok(())
}
