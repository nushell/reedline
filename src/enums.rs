use crossterm::event::{Event, KeyEvent, KeyEventKind};
use serde::{Deserialize, Serialize};
use strum::{EnumDiscriminants, EnumIter, EnumString, VariantArray};

/// Which mouse button was pressed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseButton {
    /// Left mouse button
    #[default]
    Left,
    /// Right mouse button
    Right,
    /// Middle mouse button
    Middle,
}

impl From<crossterm::event::MouseButton> for MouseButton {
    fn from(button: crossterm::event::MouseButton) -> Self {
        match button {
            crossterm::event::MouseButton::Left => Self::Left,
            crossterm::event::MouseButton::Right => Self::Right,
            crossterm::event::MouseButton::Middle => Self::Middle,
        }
    }
}

/// Valid ways how `Reedline::read_line()` can return
#[non_exhaustive]
#[derive(Debug)]
pub enum Signal {
    /// Entry succeeded with the provided content
    Success(String),
    /// Entry was aborted with `Ctrl+C`
    CtrlC, // Interrupt current editing
    /// Abort with `Ctrl+D` signalling `EOF` or abort of a whole interactive session
    CtrlD, // End terminal session

    /// A custom, uninterpreted payload passed back to the host application.
    ///
    /// This signal is triggered by a [`ReedlineEvent::ExecuteHostCommand`].
    /// The contained string is a "passthrough" value that Reedline does not
    /// inspect or modify; it is up to the caller to define the protocol
    /// and execution logic for this payload.
    HostCommand(String),

    /// An external signal requested that `read_line()` return.
    /// Contains the current buffer contents at the time of interruption.
    ExternalBreak(String),
}

/// Scope of text object operation ("i" inner or "a" around)
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum TextObjectScope {
    /// Just the text object itself
    Inner,
    /// Expanded to include surrounding based on object type
    Around,
}

/// Text object quote types
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum TextObjectQuote {
    /// '
    SingleQuote,
    /// "
    DoubleQuote,
    /// \`
    Tick,
    /// (, ), \[, ], {, }, <, >
    All,
}

/// Text object bracket types
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum TextObjectBracket {
    /// (, )
    Parenthesis,
    /// \[, ]
    SquareBracket,
    /// {, }
    CurlyBracket,
    /// <, >
    AngleBracket,
    /// (, ), \[, ], {, }, <, >
    All,
}

/// Type of text object to operate on
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum TextObjectType {
    /// word (delimited by non-alphanumeric characters)
    Word,
    /// WORD (delimited only by whitespace)
    BigWord,
    /// Brackets pairs (`(`, `)`, `[`, `]`, `{`, `}`, `<`, `>`)
    Brackets(TextObjectBracket),
    /// Quotes pairs (`"`, `'`, `\``)
    Quotes(TextObjectQuote),
}

/// Text objects that can be operated on with vim-style commands
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct TextObject {
    /// Whether to include surrounding context
    pub scope: TextObjectScope,
    /// The type of text object
    pub object_type: TextObjectType,
}

impl Default for TextObject {
    fn default() -> Self {
        Self {
            scope: TextObjectScope::Inner,
            object_type: TextObjectType::Word,
        }
    }
}

/// Direction a cursor motion travels through the buffer.
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Toward the end of the buffer (right / `w` / `$`).
    Forward,
    /// Toward the start of the buffer (left / `b` / `0`).
    Backward,
}

impl Direction {
    pub(crate) fn reversed(self) -> Self {
        match self {
            Direction::Forward => Direction::Backward,
            Direction::Backward => Direction::Forward,
        }
    }
}

/// Which "word" notion a word motion uses.
///
/// The flavors differ only in which character-class transitions count as a
/// boundary; see the classifier in `core_editor::word`.
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum WordKind {
    /// `w`/`b`/`e` — a boundary at any character-class change.
    Word,
    /// `W`/`B`/`E` — a boundary only at whitespace/line-ending, so a run like
    /// `foo.bar` is one WORD.
    LongWord,
    /// Emacs `M-f`/`M-b` — Unicode (UAX-29) word segmentation, so e.g. `can't`
    /// and `3.14` stay single words. The one flavor that isn't a thin char-class
    /// predicate; see `locate_word`. Holding those together turns on the
    /// characters flanking a `'` or a `.` rather than on their class, thus it
    /// stays a separate scan instead of collapsing onto a boundary predicate.
    Unicode,
}

/// Which edge of a word a motion lands on.
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum WordEdge {
    /// First character of the word — `w`/`W` (forward), `b`/`B` (backward).
    Start,
    /// Last character of the word, inclusive — `e`/`E`.
    End,
}

/// Where a character-search motion stops relative to the found character.
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum FindStop {
    /// Land on the found character — vi `f`/`F`.
    On,
    /// Land just before it — vi `t`/`T`.
    Before,
}

/// Granularity of an operator and of the register it fills: inline characters or
/// whole lines.
///
/// `LineWise` (vi `dd`/`yy`/`V`) operates on complete lines and pastes on a new
/// line below/above; `CharWise` is the default inline behavior. Carried on the
/// operator and stored on the cut buffer, so paste knows which to do.
///
/// Non-exhaustive: granularities may be added (e.g. block-wise), so external
/// matches need a wildcard arm.
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum Granularity {
    /// Inline at the cursor — the default.
    #[default]
    CharWise,
    /// Whole lines, pasted below/above.
    LineWise,
}

/// A human-readable, parameterized motion target — the public vocabulary every
/// cursor motion lowers from.
///
/// `Move`/`Extend`/`Cut`/`Copy`/`Erase` over a `MotionTarget` are the
/// going-forward motion API. They resolve through the private `resolve_motion`
/// and apply to the cursor, both free to change. Mode differences (vi vs emacs
/// vs helix word rules) are carried as *data* here (e.g. [`WordKind`]) rather
/// than as separate commands.
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum MotionTarget {
    /// One grapheme in `direction` — `MoveLeft`/`MoveRight`.
    Grapheme(Direction),
    /// A word edge — vi `w`/`W`/`e`/`E`/`b`/`B`.
    Word {
        /// Small word vs big WORD.
        kind: WordKind,
        /// First vs last character of the word.
        edge: WordEdge,
        /// Travel direction.
        direction: Direction,
    },
    /// Logical line edge: `Backward` = line start (`0`), `Forward` = line end (`$`).
    LineEdge(Direction),
    /// First non-whitespace character on the current line (helix `gs`). A blank
    /// line has none, so the motion stays put.
    LineStartNonBlank,
    /// Whole-buffer edge: `Backward` = start (`gg`), `Forward` = end (`G`).
    BufferEdge(Direction),
    /// The adjacent logical line: `Forward` = line below (`j`), `Backward` =
    /// line above (`k`). Used by the linewise operators (`dj`/`dk`); the head
    /// lands on the adjacent line so a `LineWise` span covers both lines.
    Line(Direction),
    /// Character search — vi `f`/`F`/`t`/`T`.
    Find {
        /// The character to search for.
        ch: char,
        /// Travel direction.
        direction: Direction,
        /// Land on the character vs just before it.
        stop: FindStop,
    },
    /// A byte position, clamped into the buffer — measured from the buffer
    /// start, not a displacement from the cursor.
    Position(usize),
}

impl MotionTarget {
    /// The `,`-style reverse: flip a [`Find`](MotionTarget::Find)'s direction.
    ///
    /// Only `Find` is reversible — every other target passes through unchanged,
    /// because a reversed word/line edge is a *different* motion, not the same
    /// motion the other way (e.g. backward word-end is `ge`, not `e` flipped).
    pub(crate) fn reversed(self) -> Self {
        match self {
            MotionTarget::Find {
                ch,
                direction,
                stop,
            } => MotionTarget::Find {
                ch,
                direction: direction.reversed(),
                stop,
            },
            other => other,
        }
    }

    /// Which way the motion travels, or `None` for a target that names a
    /// destination rather than a displacement.
    pub(crate) fn direction(self) -> Option<Direction> {
        match self {
            MotionTarget::Grapheme(direction)
            | MotionTarget::Word { direction, .. }
            | MotionTarget::LineEdge(direction)
            | MotionTarget::BufferEdge(direction)
            | MotionTarget::Line(direction)
            | MotionTarget::Find { direction, .. } => Some(direction),
            // Destination-shaped targets go here. Where they lie depends on the
            // cursor, so callers resolve first and compare after.
            MotionTarget::Position(_) => None,
            MotionTarget::LineStartNonBlank => None,
        }
    }
}

/// Editing actions which can be mapped to key bindings.
///
/// Executed by `Reedline::run_edit_commands()`
#[non_exhaustive]
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq, EnumDiscriminants)]
#[strum_discriminants(doc = "This is the auto generated discriminant type for [`EditCommand`]")]
#[strum_discriminants(derive(EnumIter, EnumString, VariantArray))]
#[strum_discriminants(strum(ascii_case_insensitive))]
pub enum EditCommand {
    /// Move to the start of the buffer
    MoveToStart {
        /// Select the text between the current cursor position and destination
        select: bool,
    },

    /// Move to the start of the current line
    MoveToLineStart {
        /// Select the text between the current cursor position and destination
        select: bool,
    },

    /// Move to the start of the current line skipping any whitespace
    MoveToLineNonBlankStart {
        /// Select the text between the current cursor position and destination
        select: bool,
    },

    /// Move to the end of the buffer
    MoveToEnd {
        /// Select the text between the current cursor position and destination
        select: bool,
    },

    /// Move to the end of the current line
    MoveToLineEnd {
        /// Select the text between the current cursor position and destination
        select: bool,
    },

    /// Move one line up
    MoveLineUp {
        /// Select the text between the current cursor position and destination
        select: bool,
    },

    /// Move one line down
    MoveLineDown {
        /// Select the text between the current cursor position and destination
        select: bool,
    },

    /// Move one character to the left
    MoveLeft {
        /// Select the text between the current cursor position and destination
        select: bool,
    },

    /// Move one character to the right
    MoveRight {
        /// Select the text between the current cursor position and destination
        select: bool,
    },

    /// Move one word to the left
    MoveWordLeft {
        /// Select the text between the current cursor position and destination
        select: bool,
    },

    /// Move one WORD to the left
    MoveBigWordLeft {
        /// Select the text between the current cursor position and destination
        select: bool,
    },

    /// Move one word to the right
    MoveWordRight {
        /// Select the text between the current cursor position and destination
        select: bool,
    },

    /// Move one word to the right, stop at start of word
    MoveWordRightStart {
        /// Select the text between the current cursor position and destination
        select: bool,
    },

    /// Move one WORD to the right, stop at start of WORD
    MoveBigWordRightStart {
        /// Select the text between the current cursor position and destination
        select: bool,
    },

    /// Move one word to the right, stop at end of word
    MoveWordRightEnd {
        /// Select the text between the current cursor position and destination
        select: bool,
    },

    /// Move one WORD to the right, stop at end of WORD
    MoveBigWordRightEnd {
        /// Select the text between the current cursor position and destination
        select: bool,
    },

    /// Move to position
    MoveToPosition {
        /// Position to move to
        position: usize,
        /// Select the text between the current cursor position and destination
        select: bool,
    },

    /// Move the cursor to a [`MotionTarget`], collapsing any selection.
    ///
    /// The parameterized, human-readable face of the selection primitive.
    /// `Move`/`Extend`/`Cut`/`Copy`/`Change`/`Erase` over a [`MotionTarget`] are
    /// the going-forward motion API; the older `MoveWord*`/`CutWord*` variants
    /// are kept as sugar over them.
    Move(MotionTarget),

    /// Extend the selection to a [`MotionTarget`]: move the cursor head to the
    /// target while keeping the anchor fixed.
    Extend(MotionTarget),

    /// Cut the text between the cursor and a [`MotionTarget`] into the cut buffer.
    /// `granularity` decides whether the span is taken char-wise or snapped to
    /// whole lines (and tags the register accordingly).
    Cut {
        /// Where the operator reaches to.
        target: MotionTarget,
        /// Char-wise span or whole lines.
        granularity: Granularity,
    },

    /// Copy the text between the cursor and a [`MotionTarget`] into the cut
    /// buffer, leaving the buffer and cursor unchanged.
    Copy {
        /// Where the operator reaches to.
        target: MotionTarget,
        /// Char-wise span or whole lines.
        granularity: Granularity,
    },

    /// Select up to a [`MotionTarget`]: drop a fresh anchor at the caret, then
    /// move the head to the target, so the selection covers the span just traveled.
    Select(MotionTarget),

    /// Select a [`TextObject`]
    SelectTextObject(TextObject),

    /// Cut like [`EditCommand::Cut`], except that a `LineWise` span keeps its
    /// line terminators: only the lines' *content* is consumed, so one blank
    /// line remains — the vi change operator's linewise semantics
    /// (`cc`/`cj`/`cgg`), which re-enter insert mode on that blank line.
    /// Identical to `Cut` for `CharWise` spans.
    Change {
        /// Where the operator reaches to.
        target: MotionTarget,
        /// Char-wise span or whole lines.
        granularity: Granularity,
    },

    /// Erase the text between the cursor and a [`MotionTarget`] without touching
    /// the cut buffer (no-register counterpart of [`EditCommand::Cut`]).
    Erase(MotionTarget),

    /// Insert a character at the current insertion point
    InsertChar(char),

    /// Insert a string at the current insertion point
    InsertString(String),

    /// Inserts the system specific new line character
    ///
    /// - On Unix systems LF (`"\n"`)
    /// - On Windows CRLF (`"\r\n"`)
    InsertNewline,

    /// Inserts a new line above the current line
    ///
    /// - On Unix systems LF (`"\n"`)
    /// - On Windows CRLF (`"\r\n"`)
    InsertNewlineAbove,

    /// Inserts a new line below the current line
    ///
    /// - On Unix systems LF (`"\n"`)
    /// - On Windows CRLF (`"\r\n"`)
    InsertNewlineBelow,

    /// Replace a character
    ReplaceChar(char),

    /// Replace characters with string
    ReplaceChars(usize, String),

    /// Backspace delete from the current insertion point
    Backspace,

    /// Delete in-place from the current insertion point
    Delete,

    /// Cut the grapheme left from the current insertion point
    CutCharLeft,

    /// Cut the grapheme right from the current insertion point
    CutChar,

    /// Backspace delete a word from the current insertion point
    BackspaceWord,

    /// Delete in-place a word from the current insertion point
    DeleteWord,

    /// Clear the current buffer
    Clear,

    /// Clear to the end of the current line
    ClearToLineEnd,

    /// Insert completion: entire completion if there is only one possibility, or else up to shared prefix.
    Complete,

    /// Cut the current line
    ///
    /// Legacy — prefer [`EditCommand::Cut`] with
    /// [`MotionTarget::LineEdge`]`(Forward)` and [`Granularity::LineWise`],
    /// which all builtin bindings lower through.
    CutCurrentLine,

    /// Cut from the start of the buffer to the insertion point
    CutFromStart,

    /// Cut from the start of the buffer to the line of insertion point
    ///
    /// Legacy — prefer [`EditCommand::Cut`] / [`EditCommand::Change`] (for
    /// `leave_blank_line: true`) with [`MotionTarget::BufferEdge`]`(Backward)`
    /// and [`Granularity::LineWise`], which all builtin bindings lower through.
    CutFromStartLinewise {
        /// When true, an empty line will remain after the operation
        leave_blank_line: bool,
    },

    /// Cut from the start of the current line to the insertion point
    CutFromLineStart,

    /// Cut from the first non whitespace character of the current line to the insertion point
    CutFromLineNonBlankStart,

    /// Cut from the insertion point to the end of the buffer
    CutToEnd,

    /// Cut from the line of insertion point to the end of the buffer
    ///
    /// Legacy — prefer [`EditCommand::Cut`] / [`EditCommand::Change`] (for
    /// `leave_blank_line: true`) with [`MotionTarget::BufferEdge`]`(Forward)`
    /// and [`Granularity::LineWise`], which all builtin bindings lower through.
    CutToEndLinewise {
        /// When true, an empty line will remain after the operation
        leave_blank_line: bool,
    },

    /// Cut from the insertion point to the end of the current line
    CutToLineEnd,

    /// Cut from the insertion point to the end of the current line
    /// If the cursor is already at the end of the line, remove the newline character
    KillLine,

    /// Cut the word left of the insertion point
    CutWordLeft,

    /// Cut the WORD left of the insertion point
    CutBigWordLeft,

    /// Cut the word right of the insertion point
    CutWordRight,

    /// Cut the word right of the insertion point
    CutBigWordRight,

    /// Cut the word right of the insertion point and any following space
    CutWordRightToNext,

    /// Cut the WORD right of the insertion point and any following space
    CutBigWordRightToNext,

    /// Paste the cut buffer in front of the insertion point (Emacs, vi `P`)
    PasteCutBufferBefore,

    /// Paste the cut buffer in front of the insertion point (vi `p`)
    PasteCutBufferAfter,

    /// Paste the cut buffer at the selection edge in the given `direction` and
    /// select the pasted text (helix `p`/`P`). Selecting the result means the
    /// command cannot simply be issued twice for a count, so it carries `count`
    /// explicitly.
    PasteAtSelectionEdge {
        /// Whether to paste on the forward or backward edge of the selection
        direction: Direction,
        /// Number of times the cut buffer content is placed
        count: usize,
    },

    /// Upper case the current word
    UppercaseWord,

    /// Lower case the current word
    LowercaseWord,

    /// Capitalize the current character
    CapitalizeChar,

    /// Switch the case of the current character
    SwitchcaseChar,

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
    MoveRightUntil {
        /// Char to move towards
        c: char,
        /// Select the text between the current cursor position and destination
        select: bool,
    },

    /// CutUntil right before char
    MoveRightBefore {
        /// Char to move towards
        c: char,
        /// Select the text between the current cursor position and destination
        select: bool,
    },

    /// CutUntil left until char
    CutLeftUntil(char),

    /// CutUntil left before char
    CutLeftBefore(char),

    /// Move left until char
    MoveLeftUntil {
        /// Char to move towards
        c: char,
        /// Select the text between the current cursor position and destination
        select: bool,
    },

    /// Move left before char
    MoveLeftBefore {
        /// Char to move towards
        c: char,
        /// Select the text between the current cursor position and destination
        select: bool,
    },

    /// Select whole input buffer
    SelectAll,

    /// Snap the selection out to the whole lines it touches (helix `x`), or take
    /// one more line when it already spans them exactly.
    ///
    /// Repeating therefore grows it a line at a time, so a count is the command
    /// applied that many times.
    SelectLine,

    /// Delete the selection without filling the cut buffer (helix `Alt-d`).
    ///
    /// [`CutSelection`](EditCommand::CutSelection) clobbers the register, which
    /// is exactly what this avoids when the text is not wanted back.
    EraseSelection,

    /// Cut selection to local buffer
    CutSelection {
        /// Char-wise span or whole lines.
        granularity: Granularity,
    },

    /// Collapse the selection (or a block cursor's width) to a caret at one of its edges
    CollapseSelection(Direction),

    /// Copy selection to local buffer
    CopySelection,

    /// LowercaseSelection
    LowercaseSelection,

    /// Uppercase selection
    UppercaseSelection,

    /// Switchcase selection
    SwitchcaseSelection,

    /// Paste content from local buffer at the current cursor position
    Paste,

    /// Copy from the start of the buffer to the insertion point
    CopyFromStart,

    /// Copy from the start of the buffer to the line of insertion point
    ///
    /// Legacy — prefer [`EditCommand::Copy`] with
    /// [`MotionTarget::BufferEdge`]`(Backward)` and [`Granularity::LineWise`].
    CopyFromStartLinewise,

    /// Copy from the start of the current line to the insertion point
    CopyFromLineStart,

    /// Copy from the first non whitespace character of the current line to the insertion point
    CopyFromLineNonBlankStart,

    /// Copy from the insertion point to the end of the buffer
    CopyToEnd,

    /// Copy from the line of insertion point to the end of the buffer
    ///
    /// Legacy — prefer [`EditCommand::Copy`] with
    /// [`MotionTarget::BufferEdge`]`(Forward)` and [`Granularity::LineWise`].
    CopyToEndLinewise,

    /// Copy from the insertion point to the end of the current line
    CopyToLineEnd,

    /// Copy the current line
    CopyCurrentLine,

    /// Copy the word left of the insertion point
    CopyWordLeft,

    /// Copy the WORD left of the insertion point
    CopyBigWordLeft,

    /// Copy the word right of the insertion point
    CopyWordRight,

    /// Copy the WORD right of the insertion point
    CopyBigWordRight,

    /// Copy the word right of the insertion point and any following space
    CopyWordRightToNext,

    /// Copy the WORD right of the insertion point and any following space
    CopyBigWordRightToNext,

    /// Copy one character to the left
    CopyLeft,

    /// Copy one character to the right
    CopyRight,

    /// Copy until right until char
    CopyRightUntil(char),

    /// Copy right before char
    CopyRightBefore(char),

    /// Copy left until char
    CopyLeftUntil(char),

    /// Copy left before char
    CopyLeftBefore(char),
    /// Swap the positions of the cursor and anchor
    SwapCursorAndAnchor,

    /// Cut selection to system clipboard
    #[cfg(feature = "system_clipboard")]
    CutSelectionSystem,

    /// Copy selection to system clipboard
    #[cfg(feature = "system_clipboard")]
    CopySelectionSystem,

    /// Paste content from system clipboard at the current cursor position
    #[cfg(feature = "system_clipboard")]
    PasteSystem,

    /// Delete text between matching characters atomically
    CutInsidePair {
        /// Left character of the pair
        left: char,
        /// Right character of the pair (usually matching bracket)
        right: char,
    },
    /// Yank text between matching characters atomically
    CopyInsidePair {
        /// Left character of the pair
        left: char,
        /// Right character of the pair (usually matching bracket)
        right: char,
    },
    /// Delete text around matching characters atomically (including the pair characters)
    CutAroundPair {
        /// Left character of the pair
        left: char,
        /// Right character of the pair (usually matching bracket)
        right: char,
    },
    /// Yank text around matching characters atomically (including the pair characters)
    CopyAroundPair {
        /// Left character of the pair
        left: char,
        /// Right character of the pair (usually matching bracket)
        right: char,
    },
    /// Cut the specified text object
    CutTextObject {
        /// The text object to operate on
        text_object: TextObject,
    },
    /// Copy the specified text object
    CopyTextObject {
        /// The text object to operate on
        text_object: TextObject,
    },
    /// Add the specified text object around the selection
    AddTextObject {
        /// The text object to operate on
        text_object: TextObjectType,
    },
    /// Remove the nearest specified text object around the cursor head
    RemoveTextObject {
        /// The text object to operate on
        text_object: TextObjectType,
    },
    /// Replace the nearest specified text object around the cursor head
    ReplaceTextObject {
        /// The old text object to replace
        old: TextObjectType,
        /// The new text object to replace with
        new: TextObjectType,
    },
}

impl EditCommand {
    /// Determine if a certain operation should be undoable
    /// or if the operations should be coalesced for undoing
    pub fn edit_type(&self) -> EditType {
        match self {
            // Cursor moves
            EditCommand::MoveToStart { select, .. }
            | EditCommand::MoveToEnd { select, .. }
            | EditCommand::MoveToLineStart { select, .. }
            | EditCommand::MoveToLineEnd { select, .. }
            | EditCommand::MoveToLineNonBlankStart { select, .. }
            | EditCommand::MoveToPosition { select, .. }
            | EditCommand::MoveLineUp { select, .. }
            | EditCommand::MoveLineDown { select, .. }
            | EditCommand::MoveLeft { select, .. }
            | EditCommand::MoveRight { select, .. }
            | EditCommand::MoveWordLeft { select, .. }
            | EditCommand::MoveBigWordLeft { select, .. }
            | EditCommand::MoveWordRight { select, .. }
            | EditCommand::MoveWordRightStart { select, .. }
            | EditCommand::MoveBigWordRightStart { select, .. }
            | EditCommand::MoveWordRightEnd { select, .. }
            | EditCommand::MoveBigWordRightEnd { select, .. }
            | EditCommand::MoveRightUntil { select, .. }
            | EditCommand::MoveRightBefore { select, .. }
            | EditCommand::MoveLeftUntil { select, .. }
            | EditCommand::MoveLeftBefore { select, .. } => {
                EditType::MoveCursor { select: *select }
            }
            EditCommand::SwapCursorAndAnchor => EditType::MoveCursor { select: true },

            EditCommand::SelectAll => EditType::MoveCursor { select: true },
            EditCommand::EraseSelection => EditType::EditText,
            EditCommand::SelectLine => EditType::MoveCursor { select: true },
            // Text edits
            EditCommand::InsertChar(_)
            | EditCommand::Backspace
            | EditCommand::Delete
            | EditCommand::CutChar
            | EditCommand::CutCharLeft
            | EditCommand::InsertString(_)
            | EditCommand::InsertNewline
            | EditCommand::InsertNewlineAbove
            | EditCommand::InsertNewlineBelow
            | EditCommand::ReplaceChar(_)
            | EditCommand::ReplaceChars(_, _)
            | EditCommand::BackspaceWord
            | EditCommand::DeleteWord
            | EditCommand::Clear
            | EditCommand::ClearToLineEnd
            | EditCommand::Complete
            | EditCommand::CutCurrentLine
            | EditCommand::CutFromStart
            | EditCommand::CutFromStartLinewise { .. }
            | EditCommand::CutFromLineStart
            | EditCommand::CutFromLineNonBlankStart
            | EditCommand::CutToLineEnd
            | EditCommand::KillLine
            | EditCommand::CutToEnd
            | EditCommand::CutToEndLinewise { .. }
            | EditCommand::CutWordLeft
            | EditCommand::CutBigWordLeft
            | EditCommand::CutWordRight
            | EditCommand::CutBigWordRight
            | EditCommand::CutWordRightToNext
            | EditCommand::CutBigWordRightToNext
            | EditCommand::PasteCutBufferBefore
            | EditCommand::PasteCutBufferAfter
            | EditCommand::UppercaseWord
            | EditCommand::LowercaseWord
            | EditCommand::SwitchcaseChar
            | EditCommand::CapitalizeChar
            | EditCommand::SwapWords
            | EditCommand::SwapGraphemes
            | EditCommand::CutRightUntil(_)
            | EditCommand::CutRightBefore(_)
            | EditCommand::CutLeftUntil(_)
            | EditCommand::CutLeftBefore(_)
            | EditCommand::CutSelection { .. }
            | EditCommand::LowercaseSelection
            | EditCommand::UppercaseSelection
            | EditCommand::SwitchcaseSelection
            | EditCommand::Paste
            | EditCommand::CutInsidePair { .. }
            | EditCommand::CutAroundPair { .. }
            | EditCommand::CutTextObject { .. }
            | EditCommand::PasteAtSelectionEdge { .. } => EditType::EditText,

            #[cfg(feature = "system_clipboard")] // Sadly cfg attributes in patterns don't work
            EditCommand::CutSelectionSystem | EditCommand::PasteSystem => EditType::EditText,

            EditCommand::Undo | EditCommand::Redo => EditType::UndoRedo,

            EditCommand::CopySelection => EditType::NoOp,
            #[cfg(feature = "system_clipboard")]
            EditCommand::CopySelectionSystem => EditType::NoOp,
            EditCommand::CopyFromStart
            | EditCommand::CopyFromStartLinewise
            | EditCommand::CopyFromLineStart
            | EditCommand::CopyFromLineNonBlankStart
            | EditCommand::CopyToEnd
            | EditCommand::CopyToEndLinewise
            | EditCommand::CopyToLineEnd
            | EditCommand::CopyCurrentLine
            | EditCommand::CopyWordLeft
            | EditCommand::CopyBigWordLeft
            | EditCommand::CopyWordRight
            | EditCommand::CopyBigWordRight
            | EditCommand::CopyWordRightToNext
            | EditCommand::CopyBigWordRightToNext
            | EditCommand::CopyLeft
            | EditCommand::CopyRight
            | EditCommand::CopyRightUntil(_)
            | EditCommand::CopyRightBefore(_)
            | EditCommand::CopyLeftUntil(_)
            | EditCommand::CopyLeftBefore(_)
            | EditCommand::CopyInsidePair { .. }
            | EditCommand::CopyAroundPair { .. }
            | EditCommand::CopyTextObject { .. } => EditType::NoOp,

            EditCommand::AddTextObject { .. }
            | EditCommand::RemoveTextObject { .. }
            | EditCommand::ReplaceTextObject { .. } => EditType::MoveCursor { select: true },

            // The six MotionTarget verbs. `Move`/`Extend` carry the old `select`
            // bool in the verb itself (Extend must be `select: true` so the editor
            // does not clear the selection it is extending). `Cut`/`Change`/`Erase`
            // mutate the buffer; `Copy` does not.
            EditCommand::Move(_) => EditType::MoveCursor { select: false },
            EditCommand::Extend(_) => EditType::MoveCursor { select: true },
            EditCommand::CollapseSelection(_) => EditType::MoveCursor { select: false },
            EditCommand::Select(_) | EditCommand::SelectTextObject(_) => {
                EditType::MoveCursor { select: true }
            }
            EditCommand::Cut { .. } => EditType::EditText,
            EditCommand::Copy { .. } => EditType::NoOp,
            EditCommand::Change { .. } => EditType::EditText,
            EditCommand::Erase(_) => EditType::EditText,
        }
    }
}

/// Specifies the types of edit commands, used to simplify grouping edits
/// to mark undo behavior
#[derive(PartialEq, Eq)]
pub enum EditType {
    /// Cursor movement commands
    MoveCursor { select: bool },
    /// Undo/Redo commands
    UndoRedo,
    /// Text editing commands
    EditText,
    /// No effect on line buffer
    NoOp,
}

/// Every line change should come with an `UndoBehavior` tag, which can be used to
/// calculate how the change should be reflected on the undo stack
#[derive(Debug)]
pub enum UndoBehavior {
    /// Character insertion, tracking the character inserted
    InsertCharacter(char),
    /// Backspace command, tracking the deleted character (left of cursor)
    /// Warning: this does not track the whole grapheme, just the character
    Backspace(Option<char>),
    /// Delete command, tracking the deleted character (right of cursor)
    /// Warning: this does not track the whole grapheme, just the character
    Delete(Option<char>),
    /// Move the cursor position
    MoveCursor,
    /// Navigated the history using up or down arrows
    HistoryNavigation,
    /// Catch-all for actions that should always form a unique undo point and never be
    /// grouped with later edits
    CreateUndoPoint,
    /// For actions that shouldn't be reflected on the edit stack e.g. Undo/Redo
    NoOp,
}

impl UndoBehavior {
    /// Return if the current operation should start a new undo set, or be
    /// combined with the previous operation
    pub fn create_undo_point_after(&self, previous: &UndoBehavior) -> bool {
        use UndoBehavior as UB;
        match (previous, self) {
            // Never start an undo set with cursor movement
            (_, UB::MoveCursor) => false,
            (UB::HistoryNavigation, UB::HistoryNavigation) => false,
            // When inserting/deleting repeatedly, each undo set should encompass
            // inserting/deleting a complete word and the associated whitespace
            (UB::InsertCharacter(c_prev), UB::InsertCharacter(c_new)) => {
                (*c_prev == '\n' || *c_prev == '\r')
                    || (!c_prev.is_whitespace() && c_new.is_whitespace())
            }
            (UB::Backspace(Some(c_prev)), UB::Backspace(Some(c_new))) => {
                (*c_new == '\n' || *c_new == '\r')
                    || (c_prev.is_whitespace() && !c_new.is_whitespace())
            }
            (UB::Backspace(_), UB::Backspace(_)) => false,
            (UB::Delete(Some(c_prev)), UB::Delete(Some(c_new))) => {
                (*c_new == '\n' || *c_new == '\r')
                    || (c_prev.is_whitespace() && !c_new.is_whitespace())
            }
            (UB::Delete(_), UB::Delete(_)) => false,
            (_, _) => true,
        }
    }
}

/// Reedline supported actions.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, EnumDiscriminants)]
#[strum_discriminants(doc = "This is the auto generated discriminant type for [`ReedlineEvent`]")]
#[strum_discriminants(derive(EnumIter, EnumString, VariantArray))]
#[strum_discriminants(strum(ascii_case_insensitive))]
pub enum ReedlineEvent {
    /// No op event
    None,

    /// Complete history hint (default in full)
    HistoryHintComplete,

    /// Complete a single token/word of the history hint
    HistoryHintWordComplete,

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

    /// Clears the screen and the scrollback buffer
    ///
    /// Sets the prompt back to the first line
    ClearScrollback,

    /// Handle enter event
    Enter,

    /// Handle unconditional submit event
    Submit,

    /// Submit at the end of the *complete* text, otherwise newline
    SubmitOrNewline,

    /// Esc event
    #[strum_discriminants(strum(serialize = "Esc", serialize = "Escape"))]
    Esc,

    /// Mouse click event with screen coordinates
    Mouse {
        /// Column (x) position, 0-indexed from left
        column: u16,
        /// Row (y) position, 0-indexed from top
        row: u16,
        /// Which mouse button was clicked
        button: MouseButton,
    },

    /// trigger terminal resize
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

    /// Move to the start of the buffer
    ToStart,

    /// Move to the end of the buffer
    ToEnd,

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

    /// Triggers an immediate return from [`Reedline::read_line()`](crate::Reedline::read_line) with an opaque payload.
    ///
    /// Reedline does not inspect or validate the contents of this string. It is
    /// passed directly through to the caller as a [`Signal::HostCommand`].
    /// Use this to send custom instructions or serialized data from a keybinding
    /// logic back to the main application loop.
    ExecuteHostCommand(String),

    /// Open text editor
    OpenEditor,

    /// Switch the vi state machine to a named mode (vi mode only).
    ///
    /// Accepts `normal`, `insert` or `visual`, matched case-insensitively. Any
    /// other name leaves the mode alone and reports the event inapplicable. On
    /// its own that is a keybinding that does nothing; inside an
    /// [`UntilFound`](ReedlineEvent::UntilFound) it hands the key to the next
    /// event in the list instead.
    ViChangeMode(String),

    /// Switch the helix state machine to a named mode (helix mode only).
    ///
    /// Accepts `normal`, `insert` or `select`, matched case-insensitively. Any
    /// other name leaves the mode alone and reports the event inapplicable. On
    /// its own that is a keybinding that does nothing; inside an
    /// [`UntilFound`](ReedlineEvent::UntilFound) it hands the key to the next
    /// event in the list instead.
    HelixChangeMode(String),
}

pub enum EventStatus {
    Handled,
    Inapplicable,
    Exits(Signal),
}

/// A wrapper for [crossterm::event::Event].
///
/// It ensures that the given event doesn't contain [KeyEventKind::Release]
/// (which is rejected) or [KeyEventKind::Repeat] (which is converted to
/// [KeyEventKind::Press]).
pub struct ReedlineRawEvent(Event);

impl TryFrom<Event> for ReedlineRawEvent {
    type Error = ();

    fn try_from(event: Event) -> Result<Self, Self::Error> {
        match event {
            Event::Key(KeyEvent {
                kind: KeyEventKind::Release,
                ..
            }) => Err(()),
            Event::Key(KeyEvent {
                code,
                modifiers,
                kind: KeyEventKind::Repeat,
                state,
            }) => Ok(Self(Event::Key(KeyEvent {
                code,
                modifiers,
                kind: KeyEventKind::Press,
                state,
            }))),
            other => Ok(Self(other)),
        }
    }
}

impl From<ReedlineRawEvent> for Event {
    fn from(event: ReedlineRawEvent) -> Self {
        event.0
    }
}
