//! Slot rack panel — Kontakt-style multi-slot instrument rack.
//!
//! Shows a scrollable list of slot cards. Each card has preset name, MIDI
//! channel, volume/pan sliders, solo/mute buttons, and an inline code editor.

use std::sync::{Arc, Mutex};

use nih_plug_vizia::vizia::prelude::*;

use super::{AppEvent, Data};
use crate::state::PluginState;

/// Build the slot rack panel.
pub fn build(cx: &mut Context) {
    VStack::new(cx, |cx| {
        // Header row
        HStack::new(cx, |cx| {
            Label::new(cx, "Slot Rack")
                .class("panel-title");

            Button::new(
                cx,
                |cx| cx.emit(AppEvent::AddEmptySlot),
                |cx| Label::new(cx, "+ Add Slot"),
            )
            .class("add-slot-btn");
        })
        .height(Auto)
        .col_between(Pixels(8.0));

        // Separator
        Element::new(cx)
            .height(Pixels(1.0))
            .width(Stretch(1.0))
            .background_color(Color::rgb(49, 50, 68));

        // Scrollable slot list
        ScrollView::new(cx, 0.0, 0.0, false, true, |cx| {
            // Re-read plugin state to build slot cards
            Binding::new(cx, Data::plugin_state, |cx, ps_lens| {
                let ps_arc = ps_lens.get(cx);
                build_slot_list(cx, &ps_arc);
            });
        })
        .height(Stretch(1.0));
    })
    .id("slot-rack");
}

/// Build the list of slot cards from a snapshot of PluginState.
fn build_slot_list(cx: &mut Context, ps_arc: &Arc<Mutex<PluginState>>) {
    let Ok(ps) = ps_arc.lock() else { return };

    if ps.slot_configs.is_empty() {
        drop(ps);
        Label::new(cx, "No slots. Click '+ Add Slot' to get started.")
            .class("empty-message");
        return;
    }

    let slots: Vec<_> = ps.slot_configs.iter().cloned().collect();
    drop(ps);

    for (idx, config) in slots.iter().enumerate() {
        build_slot_card(cx, idx, config);
    }
}

/// Build a single slot card.
fn build_slot_card(cx: &mut Context, idx: usize, config: &crate::state::SlotConfig) {
    let name = if let Some(ref preset_id) = config.preset_id {
        preset_id.clone()
    } else if !config.source_code.is_empty() {
        "Source".to_string()
    } else {
        "Empty".to_string()
    };

    let ch_text = if config.midi_channel == 0 {
        "All".to_string()
    } else {
        format!("Ch:{}", config.midi_channel)
    };

    let is_solo = config.solo;
    let is_muted = config.muted;
    let source = config.source_code.clone();
    let compile_error = config.compile_error.clone();

    VStack::new(cx, move |cx| {
        // Header row: number, name, channel, buttons
        HStack::new(cx, move |cx| {
            Label::new(cx, &format!("{}.", idx + 1))
                .class("slot-number");
            Label::new(cx, &name)
                .class("slot-name");
            Label::new(cx, &ch_text)
                .class("slot-channel");

            HStack::new(cx, move |cx| {
                // Mute
                Button::new(
                    cx,
                    move |cx| cx.emit(AppEvent::ToggleMute(idx)),
                    |cx| Label::new(cx, "M"),
                )
                .class("slot-btn")
                .class("mute")
                .checked(is_muted);

                // Solo
                Button::new(
                    cx,
                    move |cx| cx.emit(AppEvent::ToggleSolo(idx)),
                    |cx| Label::new(cx, "S"),
                )
                .class("slot-btn")
                .class("solo")
                .checked(is_solo);

                // Remove
                Button::new(
                    cx,
                    move |cx| cx.emit(AppEvent::RemoveSlot(idx)),
                    |cx| Label::new(cx, "\u{2715}"),
                )
                .class("slot-btn")
                .class("remove");
            })
            .class("slot-buttons");
        })
        .class("slot-header");

        // Code editor
        if !source.is_empty() || compile_error.is_some() {
            let source_owned = source.clone();
            // Show source snippet as a label (full editing requires a proper
            // code editor view — for now display the first few lines)
            let preview = if source_owned.len() > 200 {
                format!("{}\u{2026}", &source_owned[..200])
            } else {
                source_owned
            };
            Label::new(cx, &preview)
                .class("code-editor")
                .width(Stretch(1.0));
        }

        // Compile error
        if let Some(ref err) = compile_error {
            Label::new(cx, err)
                .color(Color::rgb(243, 139, 168))
                .font_size(11.0);
        }
    })
    .class("slot-card");
}

/// Convert a MIDI note number to a name (e.g., 60 → "C4").
#[allow(dead_code)]
fn note_name(note: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let octave = (note as i32 / 12) - 1;
    let name = NAMES[(note % 12) as usize];
    format!("{}{}", name, octave)
}
