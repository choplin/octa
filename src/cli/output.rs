use std::io;

use urushi::{
    AnsiRenderer, ComponentRole, ComponentStyles, Line, SemanticTokens, Style, Table, TableStyle,
    TerminalProfile, Theme, View,
};

use urushi::Color;

#[derive(Debug, Clone, Copy)]
pub(crate) enum Tone {
    Body,
    Accent,
    Success,
    Warning,
}

impl Tone {
    const fn role(self) -> ComponentRole {
        match self {
            Self::Body => ComponentRole::Body,
            Self::Accent => ComponentRole::Accent,
            Self::Success => ComponentRole::Success,
            Self::Warning => ComponentRole::Warning,
        }
    }
}

pub(crate) struct Output {
    renderer: AnsiRenderer,
    theme: Theme,
}

impl Output {
    pub(crate) fn stdout() -> Self {
        let stdout = io::stdout();
        Self::new(TerminalProfile::detect_for(&stdout))
    }

    fn new(profile: TerminalProfile) -> Self {
        let tokens = SemanticTokens {
            text: Color::WHITE,
            text_muted: Color::BRIGHT_BLACK,
            background: Color::BLACK,
            surface: Color::BLACK,
            accent: Color::Rgb(56, 189, 248),
            accent_text: Color::BLACK,
            success: Color::Rgb(74, 222, 128),
            warning: Color::Rgb(250, 204, 21),
            error: Color::Rgb(248, 113, 113),
            border: Color::BRIGHT_BLACK,
        };
        let components = ComponentStyles::from_tokens(&tokens)
            .with_style(ComponentRole::Body, Style::new())
            .with_style(ComponentRole::Muted, Style::new().dim())
            .with_table(TableStyle::new(
                Style::new().foreground(tokens.accent).bold(),
                Style::new(),
                Style::new().foreground(tokens.border),
            ));

        Self {
            renderer: AnsiRenderer::new(profile),
            theme: Theme::new(tokens, components),
        }
    }

    pub(crate) fn line(&self, tone: Tone, text: impl Into<String>) -> Line {
        Line::styled(text, self.theme.style(tone.role()))
    }

    pub(crate) fn row(&self, identifier: impl Into<String>, remainder: impl Into<String>) -> Line {
        Line::new()
            .span(identifier, self.theme.style(ComponentRole::Accent))
            .span(remainder, self.theme.style(ComponentRole::Body))
    }

    pub(crate) fn field(&self, label: impl Into<String>, value: impl Into<String>) -> Line {
        Line::new()
            .span(label, self.theme.style(ComponentRole::Muted))
            .span(value, self.theme.style(ComponentRole::Body))
    }

    pub(crate) fn table<H, HS, I, R, S>(&self, headers: H, rows: I) -> View
    where
        H: IntoIterator<Item = HS>,
        HS: Into<String>,
        I: IntoIterator<Item = R>,
        R: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let table = Table::new().headers(headers).rows(rows);
        self.theme.components().table().view(&table)
    }

    pub(crate) fn print(&self, view: View) {
        println!("{}", self.renderer.render(&view));
    }
}

#[cfg(test)]
mod tests {
    use urushi::{visible_width, AnsiPolicy, ColorProfile};

    use super::*;

    #[test]
    fn disabled_ansi_preserves_visible_cli_text() {
        let output = Output::new(TerminalProfile::new(
            ColorProfile::TrueColor,
            AnsiPolicy::Disabled,
        ));
        let view = View::line(output.row("#12", " Fix output"))
            .push(output.field("state: ", "open"))
            .push(output.line(Tone::Success, "updated issue #12"));

        assert_eq!(
            output.renderer.render(&view),
            "#12 Fix output\nstate: open\nupdated issue #12"
        );
    }

    #[test]
    fn terminal_profiles_degrade_the_same_semantic_view() {
        let render = |color_profile| {
            let output = Output::new(TerminalProfile::new(color_profile, AnsiPolicy::Enabled));
            output
                .renderer
                .render(&View::line(output.line(Tone::Accent, "#12")))
        };

        assert!(render(ColorProfile::TrueColor).starts_with("\u{1b}[1;38;2;56;189;248m"));
        assert!(render(ColorProfile::Ansi256).starts_with("\u{1b}[1;38;5;"));
        assert!(render(ColorProfile::Ansi16).starts_with("\u{1b}["));
        assert_eq!(render(ColorProfile::Monochrome), "\u{1b}[1m#12\u{1b}[0m");
    }

    #[test]
    fn table_preserves_cjk_width_without_ansi() {
        let output = Output::new(TerminalProfile::new(
            ColorProfile::TrueColor,
            AnsiPolicy::Disabled,
        ));
        let rendered = output.renderer.render(&output.table(
            ["Issue", "Title"],
            [["#12".to_owned(), "日本語".to_owned()]],
        ));
        let widths = rendered.lines().map(visible_width).collect::<Vec<_>>();

        assert!(rendered.starts_with('┌'));
        assert!(rendered.contains("│ Issue"));
        assert!(rendered.contains("日本語"));
        assert!(!rendered.contains('\u{1b}'));
        assert!(widths.windows(2).all(|pair| pair[0] == pair[1]));
    }
}
