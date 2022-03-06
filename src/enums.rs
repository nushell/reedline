use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

/// Valid ways how `Reedline::read_line()` can return
#[derive(Debug)]
pub enum Signal {
    /// Entry succeeded with the provided content
    Success(String),
    /// Entry was aborted with `Ctrl+C`
    CtrlC, // Interrupt current editing
    /// Abort with `Ctrl+D` signalling `EOF` or abort of a whole interactive session
    CtrlD, // End terminal session
    /// Signal to clear the current screen. Buffer content remains untouched.
    CtrlL, // FormFeed/Clear current screen
}

/// Editing actions which can be mapped to key bindings.
///
/// Executed by `Reedline::run_edit_commands()`
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq, EnumIter)]
pub enum EditCommand {
    /// Move to the start of the buffer
    MoveToStart,

    /// Move to the start of the current line
    MoveToLineStart,

    /// Move to the end of the buffer
    MoveToEnd,

    /// Move to the end of the current line
    MoveToLineEnd,

    /// Move one character to the left
    MoveLeft,

    /// Move one character to the right
    MoveRight,

    /// Move one word to the left
    MoveWordLeft,

    /// Move one word to the right
    MoveWordRight,

    /// Move to position
    MoveToPosition(usize),

    /// Insert a character at the current insertion point
    InsertChar(char),

    /// Insert a string at the current insertion point
    InsertString(String),

    /// Repace characters with string
    ReplaceChars(usize, String),

    /// Backspace delete from the current insertion point
    Backspace,

    /// Delete in-place from the current insertion point
    Delete,

    /// Backspace delete a word from the current insertion point
    BackspaceWord,

    /// Delete in-place a word from the current insertion point
    DeleteWord,

    /// Clear the current buffer
    Clear,

    /// Clear the current buffer
    ClearToLineEnd,

    /// Cut the current line
    CutCurrentLine,

    /// Cut from the start of the buffer to the insertion point
    CutFromStart,

    /// Cut from the start of the current line to the insertion point
    CutFromLineStart,

    /// Cut from the insertion point to the end of the buffer
    CutToEnd,

    /// Cut from the insertion point to the end of the current line
    CutToLineEnd,

    /// Cut the word left of the insertion point
    CutWordLeft,

    /// Cut the word right of the insertion point
    CutWordRight,

    /// Paste the cut buffer in front of the insertion point (Emacs, vi `P`)
    PasteCutBufferBefore,

    /// Paste the cut buffer in front of the insertion point (vi `p`)
    PasteCutBufferAfter,

    /// Upper case the current word
    UppercaseWord,

    /// Lower case the current word
    LowercaseWord,

    /// Capitalize the current character
    CapitalizeChar,

    /// Swap the current word with the word to the right
    SwapWords,

    /// Swap the current grapheme/character with the one to the right
    SwapGraphemes,

    /// Undo the previous edit command
    Undo,

    /// Redo an edit command from the undo history
    Redo,

    /// CutUntil right until char
    CutRightUntil(char),

    /// CutUntil right before char
    CutRightBefore(char),

    /// CutUntil right until char
    MoveRightUntil(char),

    /// CutUntil right before char
    MoveRightBefore(char),

    /// CutUntil left until char
    CutLeftUntil(char),

    /// CutUntil left before char
    CutLeftBefore(char),

    /// CutUntil left until char
    MoveLeftUntil(char),

    /// CutUntil left before char
    MoveLeftBefore(char),
}

impl EditCommand {
    /// Determine if a certain operation should be undoable
    /// or if the operations should be coalesced for undoing
    pub fn undo_behavior(&self) -> UndoBehavior {
        match self {
            // Cursor moves
            EditCommand::MoveToStart
            | EditCommand::MoveToEnd
            | EditCommand::MoveToLineStart
            | EditCommand::MoveToLineEnd
            | EditCommand::MoveToPosition(_)
            | EditCommand::MoveLeft
            | EditCommand::MoveRight
            | EditCommand::MoveWordLeft
            | EditCommand::MoveWordRight
            | EditCommand::MoveRightUntil(_)
            | EditCommand::MoveRightBefore(_)
            | EditCommand::MoveLeftUntil(_)
            | EditCommand::MoveLeftBefore(_) => UndoBehavior::Full,

            // Coalesceable insert
            EditCommand::InsertChar(_) => UndoBehavior::Coalesce,

            // Full edits
            EditCommand::Backspace
            | EditCommand::Delete
            | EditCommand::InsertString(_)
            | EditCommand::ReplaceChars(_, _)
            | EditCommand::BackspaceWord
            | EditCommand::DeleteWord
            | EditCommand::Clear
            | EditCommand::ClearToLineEnd
            | EditCommand::CutCurrentLine
            | EditCommand::CutFromStart
            | EditCommand::CutFromLineStart
            | EditCommand::CutToLineEnd
            | EditCommand::CutToEnd
            | EditCommand::CutWordLeft
            | EditCommand::CutWordRight
            | EditCommand::PasteCutBufferBefore
            | EditCommand::PasteCutBufferAfter
            | EditCommand::UppercaseWord
            | EditCommand::LowercaseWord
            | EditCommand::CapitalizeChar
            | EditCommand::SwapWords
            | EditCommand::SwapGraphemes
            | EditCommand::CutRightUntil(_)
            | EditCommand::CutRightBefore(_)
            | EditCommand::CutLeftUntil(_)
            | EditCommand::CutLeftBefore(_) => UndoBehavior::Full,

            EditCommand::Undo | EditCommand::Redo => UndoBehavior::Ignore,
        }
    }
}

/// Specifies how the (previously executed) operation should be treated in the Undo stack.
pub enum UndoBehavior {
    /// Operation is not affecting the LineBuffers content and should be ignored
    ///
    /// e.g. the undo commands themselves are not stored in the undo stack
    Ignore,
    /// The operation is one logical unit of work that should be stored in the undo stack
    Full,
    /// The operation is a single operation that should be best coalesced in logical units such as words
    ///
    /// e.g. insertion of characters by typing
    Coalesce,
}

/// Reedline supported actions.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, EnumIter)]
pub enum ReedlineEvent {
    /// No op event
    None,

    /// Complete history hint (default in full)
    HistoryHintComplete,

    /// Complete a single token/word of the history hint
    HistoryHintWordComplete,

    /// Action event
    ActionHandler,

    /// Handle EndOfLine event
    ///
    /// Expected Behavior:
    ///
    /// - On empty line breaks execution to exit with [`Signal::CtrlD`]
    /// - Secondary behavior [`EditCommand::Delete`]
    CtrlD,

    /// Handle SIGTERM key input
    ///
    /// Expected behavior:
    ///
    /// Abort entry
    /// Run [`EditCommand::Clear`]
    /// Clear the current undo
    /// Bubble up [`Signal::CtrlC`]
    CtrlC,

    /// Clears the screen and sets prompt to first line
    ClearScreen,

    /// Handle enter event
    Enter,

    /// Esc event
    Esc,

    /// Mouse
    Mouse, // Fill in details later

    /// trigger termimal resize
    Resize(u16, u16),

    /// Run these commands in the editor
    Edit(Vec<EditCommand>),

    /// Trigger full repaint
    Repaint,

    /// Navigate to the previous historic buffer
    PreviousHistory,

    /// Move up to the previous line, if multiline, or up into the historic buffers
    Up,

    /// Move down to the next line, if multiline, or down through the historic buffers
    Down,

    /// Move right to the next column, completion entry, or complete hint
    Right,

    /// Move left to the next column, or completion entry
    Left,

    /// Navigate to the next historic buffer
    NextHistory,

    /// Search the history for a string
    SearchHistory,

    /// In vi mode multiple reedline events can be chained while parsing the
    /// command or movement characters
    Multiple(Vec<ReedlineEvent>),

    /// Test
    UntilFound(Vec<ReedlineEvent>),

    /// Trigger a menu event. It activates a menu with the event name
    Menu(String),

    /// Next element in the menu
    MenuNext,

    /// Previous element in the menu
    MenuPrevious,

    /// Moves up in the menu
    MenuUp,

    /// Moves down in the menu
    MenuDown,

    /// Moves left in the menu
    MenuLeft,

    /// Moves right in the menu
    MenuRight,

    /// Move to the next history page
    MenuPageNext,

    /// Move to the previous history page
    MenuPagePrevious,

    /// Way to bind the execution of a whole command (directly returning from [`crate::Reedline::read_line()`]) to a keybinding
    ExecuteHostCommand(String),
}

pub(crate) enum EventStatus {
    Handled,
    Inapplicable,
    Exits(Signal),
}
