// Create a reedline object with a prompt that paints a footer below the input.
// cargo run --example prompt_footer
//
// The footer is context that stays on screen while you edit: here, how many
// lines were entered so far and how to leave. It sits on the rows below the
// buffer, under a status line if there is one, and above any menu.
// "a" [Tab] opens the completion menu below the footer.
// [Enter] erases the footer before the line is submitted.

use nu_ansi_term::{Color, Style};
use reedline::{
    default_emacs_keybindings, ColumnarMenu, DefaultCompleter, DefaultPrompt, Emacs, KeyCode,
    KeyModifiers, MenuBuilder, Prompt, PromptEditMode, PromptHistorySearch, Reedline,
    ReedlineEvent, ReedlineMenu, Signal,
};
use std::{borrow::Cow, io};

/// [`DefaultPrompt`] with a footer. For custom footers, implement
/// [`Prompt::render_prompt_footer`]: reedline asks for it on every repaint and
/// draws each `\n`-separated line on its own row.
struct FooterPrompt {
    prompt: DefaultPrompt,
    entered: usize,
}

impl Prompt for FooterPrompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
        self.prompt.render_prompt_left()
    }

    fn render_prompt_right(&self) -> Cow<'_, str> {
        self.prompt.render_prompt_right()
    }

    fn render_prompt_indicator(&self, mode: PromptEditMode) -> Cow<'_, str> {
        self.prompt.render_prompt_indicator(mode)
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<'_, str> {
        self.prompt.render_prompt_multiline_indicator()
    }

    fn render_prompt_history_search_indicator(
        &self,
        history_search: PromptHistorySearch,
    ) -> Cow<'_, str> {
        self.prompt
            .render_prompt_history_search_indicator(history_search)
    }

    fn render_prompt_footer(&self) -> Cow<'_, str> {
        let entered = Style::new()
            .fg(Color::Cyan)
            .paint(format!("{} lines entered", self.entered));
        let help = Style::new().dimmed().paint("Ctrl-D to exit");
        format!("{entered}\n{help}").into()
    }
}

fn main() -> io::Result<()> {
    let words = ["alpha", "apple", "april", "avocado", "azure"]
        .map(String::from)
        .to_vec();
    let completer = Box::new(DefaultCompleter::new_with_wordlen(words, 1));
    let completion_menu = Box::new(ColumnarMenu::default().with_name("completion_menu"));

    let mut keybindings = default_emacs_keybindings();
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Tab,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::Menu("completion_menu".to_string()),
            ReedlineEvent::MenuNext,
        ]),
    );

    let mut line_editor = Reedline::create()
        .with_completer(completer)
        .with_menu(ReedlineMenu::EngineCompleter(completion_menu))
        .with_edit_mode(Box::new(Emacs::new(keybindings)));
    let mut prompt = FooterPrompt {
        prompt: DefaultPrompt::default(),
        entered: 0,
    };

    loop {
        let sig = line_editor.read_line(&prompt)?;
        match sig {
            Signal::Success(buffer) => {
                prompt.entered += 1;
                println!("We processed: {buffer}");
            }
            Signal::CtrlD | Signal::CtrlC => {
                println!("\nAborted!");
                break Ok(());
            }
            _ => {}
        }
    }
}
