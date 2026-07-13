# Default keybindings

Reedline provides Emacs and Vi editing modes. The tables below describe the
bindings returned by `default_emacs_keybindings()`,
`default_vi_insert_keybindings()`, and `default_vi_normal_keybindings()`.
Applications embedding Reedline can add, replace, or remove these bindings.

Key names use `Ctrl`, `Alt`, and `Shift` for modifiers. On some terminals,
`Alt` may be entered as `Esc` followed by the key.

## Emacs mode

### Movement and history

| Key | Action |
| --- | --- |
| `Left`, `Ctrl-b` | Move left, or move left in an open menu |
| `Right`, `Ctrl-f` | Accept a hint, move a menu right, or move right |
| `Up`, `Ctrl-p` | Move up in a menu or search backward through history |
| `Down`, `Ctrl-n` | Move down in a menu or search forward through history |
| `Ctrl-Left`, `Alt-Left`, `Alt-b` | Move one word left |
| `Ctrl-Right`, `Alt-Right`, `Alt-f` | Accept hint word or move right one word |
| `Home`, `Ctrl-a` | Move to the start of the line |
| `End`, `Ctrl-e` | Accept a history hint or move to the end of the line |
| `Ctrl-Home`, `Alt-<` | Move to the start of the buffer |
| `Ctrl-End`, `Alt->` | Move to the end of the buffer |
| `Ctrl-r` | Search history |

`Shift-Alt-,` and `Shift-Alt-.` are alternatives to `Alt-<` and `Alt->` for
terminals using the Kitty keyboard protocol.

### Editing

| Key | Action |
| --- | --- |
| `Backspace`, `Ctrl-h` | Delete the character to the left |
| `Delete` | Delete the character under the cursor |
| `Ctrl-Backspace`, `Alt-Backspace`, `Alt-m` | Delete the word to the left |
| `Ctrl-Delete`, `Alt-Delete` | Delete the word to the right |
| `Ctrl-w` | Cut the word to the left |
| `Alt-d` | Cut the word to the right |
| `Ctrl-k` | Cut from the cursor to the end of the line |
| `Ctrl-u` | Cut from the start of the line to the cursor |
| `Ctrl-y` | Paste the cut buffer before the cursor |
| `Ctrl-t` | Swap the two characters around the cursor |
| `Ctrl-z` | Undo |
| `Ctrl-g` | Redo |
| `Alt-u` | Uppercase the next word |
| `Alt-l` | Lowercase the next word |
| `Alt-c` | Capitalize the character under the cursor |
| `Shift-Enter`, `Alt-Enter` | Insert a newline without submitting |
| `Enter`, `Ctrl-j` | Submit input or insert a newline if incomplete |

### Selection

| Key | Action |
| --- | --- |
| `Shift-Left`, `Shift-Right` | Extend the selection by one character |
| `Shift-Up`, `Shift-Down` | Extend the selection by one line |
| `Ctrl-Shift-Left`, `Ctrl-Shift-Right` | Extend the selection by one word |
| `Shift-Home`, `Shift-End` | Extend to the line start or end |
| `Ctrl-Shift-Home`, `Ctrl-Shift-End` | Extend to the buffer start or end |
| `Ctrl-Shift-a` | Select the complete buffer |

When the `system_clipboard` feature is enabled, `Ctrl-Shift-x`,
`Ctrl-Shift-c`, and `Ctrl-Shift-v` cut, copy, and paste using the system
clipboard.

### Control

| Key | Action |
| --- | --- |
| `Ctrl-c` | Abort the current input |
| `Ctrl-d` | Send end-of-file |
| `Ctrl-l` | Clear the screen while preserving the current input |
| `Ctrl-o` | Open the buffer in the configured external editor |
| `Esc` | Cancel the current operation |

## Vi mode

Vi mode starts in Insert mode. `Esc` switches to Normal mode, and `v` enters
Visual mode from Normal mode. `Esc` returns from either Insert or Visual mode
to Normal mode.

### Insert mode

Insert mode supports the arrow, history, selection, control, and basic editing
bindings listed for Emacs mode. It does not include the Emacs-only character
bindings such as `Ctrl-b`, `Ctrl-f`, `Ctrl-k`, or `Alt-b`.

The following editing bindings are available in Insert mode:

| Key | Action |
| --- | --- |
| `Backspace`, `Ctrl-h` | Delete the character to the left |
| `Delete` | Delete the character under the cursor |
| `Ctrl-Backspace`, `Ctrl-w` | Delete the word to the left |
| `Ctrl-Delete` | Delete the word to the right |
| `Shift-Enter`, `Alt-Enter` | Insert a newline without submitting |
| `Enter`, `Ctrl-j` | Submit input or insert a newline if incomplete |
| `Esc` | Switch to Normal mode |

### Normal and Visual mode motions

In Visual mode, motions extend the selection. A numeric count can prefix a
motion or command, for example `3w` or `2dd`.

| Key | Action |
| --- | --- |
| `h`, `l` | Move left or right |
| `j`, `k` | Move down or up, falling back to history navigation |
| `w`, `W` | Move to the next word or whitespace-delimited word |
| `e`, `E` | Move to the end of the next word or whitespace-delimited word |
| `b`, `B` | Move to the previous word or whitespace-delimited word |
| `0`, `^`, `$` | Move to line start, first non-blank, or line end |
| `gg`, `G` | Move to the start or end of the buffer |
| `f<char>`, `F<char>` | Move to the next or previous character match |
| `t<char>`, `T<char>` | Move before the next or previous character match |
| `;`, `,` | Repeat the last character search forward or in reverse |

The arrow, `Home`/`End`, history, selection, and control bindings from Emacs
mode are also available in Normal and Visual modes. In Normal mode,
`Backspace` moves left and `Delete` deletes the character under the cursor.

### Normal mode commands

| Key | Action |
| --- | --- |
| `i`, `a` | Enter Insert mode before or after the cursor |
| `I`, `A` | Enter Insert mode at the start or end of the line |
| `o`, `O` | Open a new line below or above and enter Insert mode |
| `v` | Enter Visual mode |
| `x` | Cut the character under the cursor |
| `s` | Cut the character under the cursor and enter Insert mode |
| `r<char>` | Replace the character under the cursor |
| `C`, `S` | Change to the end of the line or change the complete line |
| `D` | Cut to the end of the line |
| `p`, `P` | Paste the cut buffer after or before the cursor |
| `u` | Undo |
| `~` | Toggle the case of the character under the cursor |
| `.` | Repeat the last editing command |
| `?` | Search history |
| `Enter` | Submit, or insert a newline and enter Insert mode if incomplete |

`d`, `c`, and `y` combine with any motion to cut, change, or copy its range.
Doubling an operator applies it to the current line: `dd`, `cc`, and `yy`.

Text objects can follow an operator. `i` selects inside an object and `a`
selects around one. The supported named objects are `w` (word), `W`
(whitespace-delimited word), `b` (brackets), and `q` (quotes). Pair delimiters
such as `(`, `[`, `{`, `<`, `"`, `'`, `` ` ``, and `$` can be used directly
with the inside form. Cut and copy also support those delimiters with the
around form.

### Visual mode commands

| Key | Action |
| --- | --- |
| `d`, `x` | Cut the selection and return to Normal mode |
| `c`, `s` | Cut the selection and enter Insert mode |
| `y` | Copy the selection and return to Normal mode |
| `p`, `P` | Replace the selection with the cut buffer (after or before) |
| `o`, `O` | Move the cursor to the other end of the selection |
| `r<char>` | Replace the selected characters and return to Normal mode |
| `Esc` | Cancel the selection and return to Normal mode |
