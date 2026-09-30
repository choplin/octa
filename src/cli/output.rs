use urushi::{
    BlockStyle, ComponentRole, ComponentTheme, SemanticTokens, Table, TableRole, TextStyle,
    TextThemeRole, Theme, VerticalAlign, View,
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
    fn style(self, theme: &Theme) -> TextStyle {
        match self {
            Self::Body => theme.text_style(ComponentRole::Body),
            Self::Accent => theme.text_style(ComponentRole::Accent),
            Self::Success => theme.text_style(ComponentRole::Success),
            Self::Warning => TextStyle::new().foreground(theme.tokens().warning),
        }
    }
}

impl TextThemeRole for Tone {
    fn resolve(self, theme: &Theme) -> TextStyle {
        self.style(theme)
    }
}

pub(crate) struct Output {
    theme: Theme,
}

impl Output {
    pub(crate) fn stdout() -> Self {
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
        let components = ComponentTheme::from_tokens(&tokens)
            .text_style(ComponentRole::Body, TextStyle::new())
            .text_style(ComponentRole::Muted, TextStyle::new().dim())
            .table_style(
                TableRole::Header,
                BlockStyle::new().foreground(tokens.accent).bold(),
            )
            .table_style(TableRole::Cell, BlockStyle::new());

        Self {
            theme: Theme::new(tokens, components),
        }
    }

    pub(crate) fn line(&self, tone: Tone, text: impl Into<String>) -> View {
        View::text(text, self.theme.text_style(tone))
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
        let table = Table::text().headers(headers).rows(rows);
        self.theme.components().table().compose(&table)
    }

    pub(crate) fn print(&self, view: View) -> std::io::Result<()> {
        urushi::println_view(&view)
    }

    /// Prints lines stacked in the order they are given.
    pub(crate) fn print_lines(&self, lines: impl IntoIterator<Item = View>) -> std::io::Result<()> {
        for line in lines {
            self.print(line)?;
        }
        Ok(())
    }
}
