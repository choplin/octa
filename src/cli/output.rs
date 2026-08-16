use std::io;

use urushi::{
    Align, AnsiRenderer, BlockStyle, ComponentRole, ComponentStyles, SemanticTokens, Table,
    TableStyle, TerminalProfile, TextStyle, Theme, VerticalAlign, View,
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
            .with_text_style(ComponentRole::Body, TextStyle::new())
            .with_text_style(ComponentRole::Muted, TextStyle::new().dim())
            .with_table(TableStyle::new(
                BlockStyle::new().foreground(tokens.accent).bold(),
                BlockStyle::new(),
                TextStyle::new().foreground(tokens.border),
            ));

        Self {
            renderer: AnsiRenderer::new(profile),
            theme: Theme::new(tokens, components),
        }
    }

    pub(crate) fn line(&self, tone: Tone, text: impl Into<String>) -> View {
        View::text(text, self.theme.text_style(tone.role()))
    }

    pub(crate) fn row(&self, identifier: impl Into<String>, remainder: impl Into<String>) -> View {
        self.spans([
            (identifier.into(), ComponentRole::Accent),
            (remainder.into(), ComponentRole::Body),
        ])
    }

    pub(crate) fn field(&self, label: impl Into<String>, value: impl Into<String>) -> View {
        self.spans([
            (label.into(), ComponentRole::Muted),
            (value.into(), ComponentRole::Body),
        ])
    }

    /// Places differently styled runs of text on one line.
    fn spans(&self, spans: impl IntoIterator<Item = (String, ComponentRole)>) -> View {
        View::row(
            VerticalAlign::Top,
            spans
                .into_iter()
                .map(|(text, role)| View::text(text, self.theme.text_style(role))),
        )
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
        println!("{}", self.render(&view));
    }

    /// Prints lines stacked in the order they are given.
    pub(crate) fn print_lines(&self, lines: impl IntoIterator<Item = View>) {
        self.print(View::column(Align::Left, lines));
    }

    /// Renders a view as the text this CLI prints.
    ///
    /// A view resolves to a rectangle, so every line is padded to the width of
    /// the widest one. CLI output is line-oriented rather than a fixed
    /// rectangle, so that padding is dropped.
    fn render(&self, view: &View) -> String {
        self.renderer
            .render(view)
            .as_str()
            .lines()
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n")
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
        let view = View::column(
            Align::Left,
            [
                output.row("#12", " Fix output"),
                output.field("state: ", "open"),
                output.line(Tone::Success, "updated issue #12"),
            ],
        );

        assert_eq!(
            output.render(&view),
            "#12 Fix output\nstate: open\nupdated issue #12"
        );
    }

    #[test]
    fn terminal_profiles_degrade_the_same_semantic_view() {
        let render = |color_profile| {
            let output = Output::new(TerminalProfile::new(color_profile, AnsiPolicy::Enabled));
            output.render(&output.line(Tone::Accent, "#12"))
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
        let rendered = output.render(&output.table(
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
