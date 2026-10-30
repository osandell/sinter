//! "What is blocking this tab?", hung under the tab like the close question,
//! with a choice of what kind of thing blocks it and a text field prefilled
//! with the tab's current note.

use gpui::{
    AnyElement, ClickEvent, Context, Entity, InteractiveElement, IntoElement, MouseDownEvent,
    ParentElement, SharedString, StatefulInteractiveElement as _, Styled, Window, div,
    prelude::FluentBuilder as _, px,
};
use ui::{
    ActiveTheme as _, Button, ButtonCommon as _, ButtonStyle, Clickable as _, StyledExt as _,
};
use ui_input::InputField;

use crate::{
    TerminalColumn,
    graphics::{self, SymbolWeight},
    terminal_column::bitmap_element,
};

pub const WIDTH: f32 = 300.;

/// What a blocked tab waits for. Shown on the tab (and in arcoscope's bar)
/// in place of the plain no-entry mark; no kind keeps that mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockedKind {
    Person,
    Clock,
    Job,
    Link,
    Lock,
    Eye,
}

impl BlockedKind {
    pub const ALL: [BlockedKind; 6] = [
        BlockedKind::Person,
        BlockedKind::Clock,
        BlockedKind::Job,
        BlockedKind::Link,
        BlockedKind::Lock,
        BlockedKind::Eye,
    ];

    /// The name in the saved session and on arcoscope's control socket.
    pub fn key(self) -> &'static str {
        match self {
            BlockedKind::Person => "person",
            BlockedKind::Clock => "clock",
            BlockedKind::Job => "job",
            BlockedKind::Link => "link",
            BlockedKind::Lock => "lock",
            BlockedKind::Eye => "eye",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.key() == key)
    }

    /// The SF Symbol (drawn from SVG on Linux, see `graphics::symbol_svg`).
    pub fn symbol(self) -> &'static str {
        match self {
            BlockedKind::Person => "person.fill",
            BlockedKind::Clock => "clock",
            // Not a gear: the tab strip's gear is a working or idle Claude.
            BlockedKind::Job => "hammer.fill",
            BlockedKind::Link => "link",
            BlockedKind::Lock => "lock.fill",
            BlockedKind::Eye => "eye",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            BlockedKind::Person => "Väntar på någon (svar, beslut, review)",
            BlockedKind::Clock => "Väntar till en viss tid",
            BlockedKind::Job => "Väntar på ett jobb (CI, bygge, deploy, träning)",
            BlockedKind::Link => "Väntar på annat arbete (en annan flik, ett scope)",
            BlockedKind::Lock => "Väntar på åtkomst eller behörighet",
            BlockedKind::Eye => "Avvaktar en händelse",
        }
    }
}

pub struct BlockedNote {
    pub tab_id: u64,
    pub kind: Option<BlockedKind>,
    pub input: Entity<InputField>,
    /// Where the panel hangs, relative to the column's top-left.
    pub x: f32,
    pub y: f32,
}

impl TerminalColumn {
    pub(crate) fn render_blocked_note(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let note = self.blocked_note()?;
        let colors = cx.theme().colors();
        let scale = window.scale_factor();
        let selected_kind = note.kind;
        let kinds = BlockedKind::ALL.into_iter().map(|kind| {
            let selected = selected_kind == Some(kind);
            let ink = if selected { colors.text } else { colors.text_muted }.to_rgb();
            div()
                .id(("ghostty-blocked-kind", kind as usize))
                .size(px(30.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_md()
                .border_1()
                .border_color(if selected {
                    colors.border_focused
                } else {
                    gpui::transparent_black()
                })
                .when(selected, |this| this.bg(colors.element_selected))
                .hover(|style| style.bg(colors.element_hover))
                .children(bitmap_element(graphics::sf_symbol(
                    kind.symbol(),
                    13.,
                    SymbolWeight::Semibold,
                    ink,
                    Some(18.),
                    0.,
                    scale,
                )))
                .tooltip(ui::Tooltip::text(SharedString::from(kind.label())))
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    this.choose_blocked_kind(kind, cx);
                }))
        });
        Some(
            gpui::deferred(
                div()
                    .id("ghostty-blocked-note")
                    .absolute()
                    .left(px(note.x))
                    .top(px(note.y))
                    .w(px(WIDTH))
                    .elevation_3(cx)
                    .p(px(12.))
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(colors.text)
                            .child("Vad blockerar fliken?"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(colors.text_muted)
                            .child(
                                "Välj vad fliken väntar på. Texten visas när du håller pekaren över ikonen; lämna den tom för att ta bort anteckningen.",
                            ),
                    )
                    .child(div().pt(px(2.)).flex().gap(px(4.)).children(kinds))
                    .child(div().pt(px(2.)).child(note.input.clone()))
                    .child(
                        div()
                            .pt(px(6.))
                            .flex()
                            .justify_end()
                            .gap(px(6.))
                            .child(Button::new("ghostty-blocked-note-cancel", "Avbryt").on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.answer_blocked_note(false, window, cx)
                                }),
                            ))
                            .child(
                                Button::new("ghostty-blocked-note-save", "Spara")
                                    .style(ButtonStyle::Filled)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.answer_blocked_note(true, window, cx)
                                    })),
                            ),
                    )
                    // The field is a single-line editor, so Enter and Esc
                    // arrive as the menu actions rather than as raw keys.
                    .on_action(cx.listener(|this, _: &menu::Confirm, window, cx| {
                        this.answer_blocked_note(true, window, cx)
                    }))
                    .on_action(cx.listener(|this, _: &menu::Cancel, window, cx| {
                        this.answer_blocked_note(false, window, cx)
                    }))
                    .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, window, cx| {
                        this.answer_blocked_note(false, window, cx)
                    })),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }
}
