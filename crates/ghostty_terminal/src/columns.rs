//! Which terminal column the window shows.
//!
//! Every workspace owns a terminal column for its worktree, but the column the
//! window shows is not always the active workspace's own. arcoscope's editor
//! follows the work: a Claude session in the `main` worktree, or a shell that
//! `cd`s, moves the editor to another worktree while the terminal stays
//! exactly where the user is. So the window pairs the *current terminal* (set
//! when arcoscope focuses a terminal) with the active workspace's editor, the way
//! arcoscope used to place a Ghostty window beside a Zed window of another path.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use gpui::{App, AppContext as _, Context, Entity, Global, WeakEntity, Window};
use workspace::{MultiWorkspace, Workspace};

use crate::TerminalColumn;

/// Which half of the window a column is drawn in. Every workspace has a
/// leading column from the start; it gets a trailing one the first time the
/// right side is put in terminal mode, and keeps it for the rest of the run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnSide {
    Leading,
    Trailing,
}

#[derive(Default)]
pub struct TerminalColumns {
    /// Each column with the workspace that owns it and the side it is drawn
    /// in. Columns are kept alive here while no workspace shows them.
    columns: Vec<(WeakEntity<Workspace>, ColumnSide, Entity<TerminalColumn>)>,
    current: Option<WeakEntity<TerminalColumn>>,
    /// The terminal column the keyboard was in last, whichever side: where
    /// arcoscope's terminal key and its dock entry land.
    last_focused: Option<WeakEntity<TerminalColumn>>,
    /// The scopes whose right side shows a terminal column instead of the
    /// editor. Kept per scope, so switching to another project finds that
    /// project's side as it was left.
    right_side_terminal: HashSet<PathBuf>,
}

impl Global for TerminalColumns {}

impl TerminalColumns {
    pub fn register(
        owner: WeakEntity<Workspace>,
        side: ColumnSide,
        column: Entity<TerminalColumn>,
        cx: &mut App,
    ) {
        cx.default_global::<TerminalColumns>()
            .columns
            .push((owner, side, column));
    }

    pub fn owners(&self) -> impl Iterator<Item = WeakEntity<Workspace>> + '_ {
        self.columns.iter().map(|(owner, _, _)| owner.clone())
    }

    pub fn unregister(owner: &WeakEntity<Workspace>, cx: &mut App) {
        let columns = cx.default_global::<TerminalColumns>();
        columns.columns.retain(|(existing, _, _)| existing != owner);
    }

    fn for_workspace(
        workspace: &WeakEntity<Workspace>,
        side: ColumnSide,
        cx: &App,
    ) -> Option<Entity<TerminalColumn>> {
        cx.try_global::<TerminalColumns>()?
            .columns
            .iter()
            .find(|(owner, own_side, _)| owner == workspace && *own_side == side)
            .map(|(_, _, column)| column.clone())
    }

    pub fn column_for_workspace(
        workspace: &WeakEntity<Workspace>,
        cx: &App,
    ) -> Option<Entity<TerminalColumn>> {
        Self::for_workspace(workspace, ColumnSide::Leading, cx)
    }

    pub fn trailing_for_workspace(
        workspace: &WeakEntity<Workspace>,
        cx: &App,
    ) -> Option<Entity<TerminalColumn>> {
        Self::for_workspace(workspace, ColumnSide::Trailing, cx)
    }

    /// The leading column of the worktree at `path` (arcoscope's window title).
    /// A trailing column shares its worktree and is never a window of its own.
    pub fn column_for_path(path: &Path, cx: &App) -> Option<Entity<TerminalColumn>> {
        cx.try_global::<TerminalColumns>()?
            .columns
            .iter()
            .find(|(_, side, column)| {
                *side == ColumnSide::Leading
                    && column
                        .read(cx)
                        .workspace_path()
                        .is_some_and(|own| own == path)
            })
            .map(|(_, _, column)| column.clone())
    }

    /// Every column, both sides: what the tab verbs and the settings that
    /// reach all terminals go over.
    pub fn all(cx: &App) -> Vec<Entity<TerminalColumn>> {
        cx.try_global::<TerminalColumns>()
            .map(|columns| {
                columns
                    .columns
                    .iter()
                    .map(|(_, _, column)| column.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The leading columns alone, one per worktree: arcoscope's windows.
    pub fn leading(cx: &App) -> Vec<Entity<TerminalColumn>> {
        cx.try_global::<TerminalColumns>()
            .map(|columns| {
                columns
                    .columns
                    .iter()
                    .filter(|(_, side, _)| *side == ColumnSide::Leading)
                    .map(|(_, _, column)| column.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Whether `column`'s tabs are part of its worktree's tab list right now.
    /// A trailing column outlives the mode that made it: its tabs stay open
    /// while the right side shows the editor again, and counting them there
    /// would let a step through the tabs land on one and turn the mode back on.
    pub fn counts_for_scope(column: &Entity<TerminalColumn>, cx: &App) -> bool {
        if Self::side_of(column, cx) == ColumnSide::Leading {
            return true;
        }
        column
            .read(cx)
            .workspace_path()
            .is_some_and(|path| Self::right_side_terminal_in(&Self::scope_root(path), cx))
    }

    pub fn side_of(column: &Entity<TerminalColumn>, cx: &App) -> ColumnSide {
        cx.try_global::<TerminalColumns>()
            .and_then(|columns| {
                columns
                    .columns
                    .iter()
                    .find(|(_, _, existing)| existing == column)
                    .map(|(_, side, _)| *side)
            })
            .unwrap_or(ColumnSide::Leading)
    }

    /// Remembers the column the keyboard went into, for the terminal key.
    pub fn set_last_focused(column: &Entity<TerminalColumn>, cx: &mut App) {
        cx.default_global::<TerminalColumns>().last_focused = Some(column.downgrade());
    }

    /// Whether `column` is the one the keyboard was last in.
    pub fn is_last_focused(column: &Entity<TerminalColumn>, cx: &App) -> bool {
        cx.try_global::<TerminalColumns>()
            .and_then(|columns| columns.last_focused.as_ref()?.upgrade())
            .is_some_and(|last| &last == column)
    }

    /// Whether the keyboard was last in a trailing column, so arcoscope's
    /// terminal key goes back to the right side rather than the left.
    pub fn last_focused_is_trailing(cx: &App) -> bool {
        let Some(columns) = cx.try_global::<TerminalColumns>() else {
            return false;
        };
        let Some(last) = columns.last_focused.as_ref().and_then(|last| last.upgrade()) else {
            return false;
        };
        Self::side_of(&last, cx) == ColumnSide::Trailing
    }

    /// The scope a worktree belongs to: the project root holding its
    /// `worktrees` directory, else the worktree itself.
    pub fn scope_root(path: &Path) -> PathBuf {
        crate::worktree_split(path)
            .map(|(container, _, _)| container)
            .unwrap_or_else(|| path.to_path_buf())
    }

    /// The scope the window shows, which is the one the terminal stands in.
    pub fn current_scope(cx: &App) -> Option<PathBuf> {
        Some(Self::scope_root(Self::current(cx)?.read(cx).workspace_path()?))
    }

    pub fn right_side_terminal_in(scope: &Path, cx: &App) -> bool {
        cx.try_global::<TerminalColumns>()
            .is_some_and(|columns| columns.right_side_terminal.contains(scope))
    }

    /// Whether the scope on screen has a terminal on its right side.
    pub fn right_side_terminal(cx: &App) -> bool {
        Self::current_scope(cx).is_some_and(|scope| Self::right_side_terminal_in(&scope, cx))
    }

    pub(crate) fn set_right_side_terminal_in(scope: PathBuf, on: bool, cx: &mut App) {
        let columns = cx.default_global::<TerminalColumns>();
        if on {
            columns.right_side_terminal.insert(scope);
        } else {
            columns.right_side_terminal.remove(&scope);
        }
    }

    pub fn current(cx: &App) -> Option<Entity<TerminalColumn>> {
        cx.try_global::<TerminalColumns>()?
            .current
            .as_ref()?
            .upgrade()
    }

    /// The trailing column shown right now: the one paired with `current`.
    pub fn current_trailing(cx: &App) -> Option<Entity<TerminalColumn>> {
        if !Self::right_side_terminal(cx) {
            return None;
        }
        let owner = Self::current(cx)?.read(cx).workspace().clone();
        Self::trailing_for_workspace(&owner, cx)
    }

    /// Shows `column` beside whatever editor the window shows.
    pub fn set_current(column: &Entity<TerminalColumn>, cx: &mut App) {
        cx.default_global::<TerminalColumns>().current = Some(column.downgrade());
        if let Some(window) = workspace::unified_window_handle(cx) {
            window
                .update(cx, |multi_workspace, window, cx| {
                    sync(multi_workspace, window, cx)
                })
                .ok();
        }
    }
}

/// Puts the current terminal next to the active workspace, hides every other
/// column and lets only the shown one draw.
pub fn sync(
    multi_workspace: &mut MultiWorkspace,
    window: &mut Window,
    cx: &mut Context<MultiWorkspace>,
) {
    let active = multi_workspace.workspace().clone();
    let owned = TerminalColumns::column_for_workspace(&active.downgrade(), cx);
    let shown = TerminalColumns::current(cx)
        .filter(|column| {
            // A column whose workspace has closed is gone with it.
            cx.try_global::<TerminalColumns>().is_some_and(|columns| {
                columns
                    .columns
                    .iter()
                    .any(|(_, _, existing)| existing == column)
            })
        })
        .or(owned);

    // The right side in terminal mode: the column beside the editor's place
    // belongs to the same worktree as the one on the left, so both halves of
    // the window show the same scope. The mode is the scope's own, so a switch
    // to another project leaves this one as it was.
    // The scope of the column about to be shown, not of the one on screen: on
    // a switch the global current is still the old one, and reading the mode
    // off it showed the scope being left's right side on the one arrived at.
    let scope = shown
        .as_ref()
        .and_then(|column| column.read(cx).workspace_path().cloned())
        .map(|path| TerminalColumns::scope_root(&path));
    let trailing = scope
        .is_some_and(|scope| TerminalColumns::right_side_terminal_in(&scope, cx))
        .then(|| {
            let owner = shown.as_ref()?.read(cx).workspace().clone();
            Some(trailing_column(&owner, window, cx))
        })
        .flatten()
        .flatten();

    let workspaces: Vec<Entity<Workspace>> = multi_workspace.workspaces().cloned().collect();
    for workspace in workspaces {
        let is_active = workspace == active;
        let column = is_active.then(|| shown.clone()).flatten();
        let trailing = is_active.then(|| trailing.clone()).flatten();
        workspace.update(cx, |workspace, cx| {
            workspace.set_leading_column(column.map(Into::into), cx);
            workspace.set_trailing_column(trailing.map(Into::into), cx);
            workspace.set_shown_in_window(is_active, window, cx);
        });
    }
    for column in TerminalColumns::all(cx) {
        let is_shown = shown.as_ref() == Some(&column) || trailing.as_ref() == Some(&column);
        column.update(cx, |column, cx| {
            column.set_workspace_active(is_shown, cx);
            if is_shown {
                column.set_displayed_in(active.downgrade());
            }
        });
    }
    if let Some(shown) = shown {
        let layout = shown.read(cx).layout();
        active.update(cx, |workspace, cx| {
            workspace.set_leading_column_layout(layout, cx)
        });
    }
}

/// The trailing column of `owner`, made on first use. A workspace that has
/// closed has none.
fn trailing_column(
    owner: &WeakEntity<Workspace>,
    window: &mut Window,
    cx: &mut App,
) -> Option<Entity<TerminalColumn>> {
    if let Some(column) = TerminalColumns::trailing_for_workspace(owner, cx) {
        return Some(column);
    }
    let project = owner.upgrade()?.read(cx).project().clone();
    let column = cx.new(|cx| {
        TerminalColumn::new(
            owner.clone(),
            crate::ColumnSide::Trailing,
            Some(project),
            window,
            cx,
        )
    });
    TerminalColumns::register(owner.clone(), ColumnSide::Trailing, column.clone(), cx);
    crate::arcoscope::watch_column(&column, cx);
    Some(column)
}
