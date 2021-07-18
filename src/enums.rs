use serde::{Deserialize, Serialize};

/// Valid ways how [`crate::Reedline::read_line()`] can return
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
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum EditCommand {
    MoveToStart,
    MoveToEnd,
    MoveLeft,
    MoveRight,
    MoveWordLeft,
    MoveWordRight,
    InsertChar(char),
    Backspace,
    Delete,
    BackspaceWord,
    DeleteWord,
    AppendToHistory,
    PreviousHistory,
    NextHistory,
    SearchHistory,
    Clear,
    CutFromStart,
    CutToEnd,
    CutWordLeft,
    CutWordRight,
    PasteCutBuffer,
    UppercaseWord,
    LowercaseWord,
    CapitalizeChar,
    SwapWords,
    SwapGraphemes,
    EnterViNormal,
    EnterViInsert,
    ViCommandFragment(char),
}

/// The edit mode [`crate::Reedline`] is currently in. Influences keybindings and prompt.
#[derive(PartialEq, Eq, Hash, Clone, Copy)]
pub enum EditMode {
    Emacs,
    ViNormal,
    ViInsert,
}
