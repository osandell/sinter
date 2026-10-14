//! "What is blocking this tab?", hung under the tab like the close question,
//! with a text field prefilled with the tab's current note.

use gpui::{
    AnyElement, Context, Entity, InteractiveElement, IntoElement, MouseDownEvent, ParentElement,
    Styled, div, px,
};
use ui::{
    ActiveTheme as _, Button, ButtonCommon as _, ButtonStyle, Clickable as _, StyledExt as _,
};
use ui_input::InputField;

use crate::TerminalColumn;

pub const WIDTH: f32 = 300.;

pub struct BlockedNote {
    pub tab_id: u64,
    pub input: Entity<InputField>,
    /// Where the panel hangs, relative to the column's top-left.
    pub x: f32,
    pub y: f32,
}

impl TerminalColumn {
    pub(crate) fn render_blocked_note(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let note = self.blocked_note()?;
        let colors = cx.theme().colors();
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
                                "Visas när du håller pekaren över blockikonen. Lämna tomt för att ta bort anteckningen.",
                            ),
                    )
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
