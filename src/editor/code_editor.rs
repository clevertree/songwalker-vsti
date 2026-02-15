use nih_plug_egui::egui;

use super::colors;

/// Draw the `.sw` code editor panel (expanded view for runner slots).
///
/// This provides syntax highlighting using songwalker-core lexer token types.
pub fn draw(ui: &mut egui::Ui, source: &mut String) {
    ui.vertical(|ui| {
        ui.label(egui::RichText::new(".sw Track Editor").color(colors::TEXT));
        ui.separator();

        // Full-size code editor
        let _response = ui.add(
            egui::TextEdit::multiline(source)
                .font(egui::TextStyle::Monospace)
                .desired_rows(20)
                .desired_width(f32::INFINITY)
                .code_editor(),
        );

        // TODO: Apply syntax highlighting overlays using songwalker_core::lexer
        // Token type → color mapping:
        //   - Note events (C4, D#5, etc.) → TEAL
        //   - Track keyword → YELLOW
        //   - const/let/for → MAUVE
        //   - String literals → GREEN
        //   - Numbers → PEACH
        //   - Comments → OVERLAY0
        //   - Operators → SUBTEXT0
        //   - Function calls → BLUE
    });
}

/// Highlight tokens in the source code (returns colored layout jobs).
///
/// This tokenizes the source using songwalker-core's lexer and maps
/// each token to a color from the Catppuccin palette.
pub fn syntax_highlight(source: &str) -> Vec<(std::ops::Range<usize>, egui::Color32)> {
    let mut highlights = Vec::new();

    // Use the core lexer to tokenize
    let mut lexer = songwalker_core::lexer::Lexer::new(source);
    match lexer.tokenize() {
        Ok(tokens) => {
            for spanned in &tokens {
                let color = match &spanned.token {
                    songwalker_core::token::Token::Track => colors::YELLOW,
                    songwalker_core::token::Token::Const
                    | songwalker_core::token::Token::Let
                    | songwalker_core::token::Token::For => colors::MAUVE,
                    songwalker_core::token::Token::Number(_) => colors::PEACH,
                    songwalker_core::token::Token::StringLit(_) => colors::GREEN,
                    songwalker_core::token::Token::Ident(name) => {
                        // Check if it looks like a note name (C, C#, D, etc. followed by octave)
                        if is_note_name(name) {
                            colors::TEAL
                        } else {
                            colors::TEXT
                        }
                    }
                    songwalker_core::token::Token::Comment(_) => colors::OVERLAY0,
                    _ => colors::SUBTEXT0,
                };

                highlights.push((spanned.span.start..spanned.span.end, color));
            }
        }
        Err(_) => {
            // If lexing fails, no highlighting
        }
    }

    highlights
}

/// Check if an identifier looks like a musical note name (C4, D#5, Eb3, etc.).
fn is_note_name(name: &str) -> bool {
    let chars: Vec<char> = name.chars().collect();
    if chars.is_empty() {
        return false;
    }

    // Must start with A-G
    if !matches!(chars[0], 'A'..='G') {
        return false;
    }

    // Optional sharp/flat
    let rest = if chars.len() > 1 && (chars[1] == '#' || chars[1] == 'b') {
        &chars[2..]
    } else {
        &chars[1..]
    };

    // Must end with a digit (octave number)
    !rest.is_empty() && rest.iter().all(|c| c.is_ascii_digit())
}
