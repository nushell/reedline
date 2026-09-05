# Default keybindings

Reedline ships three edit modes: Emacs, Vi and Helix. Each is built from a
shared pool of bindings plus its own additions. Applications embedding reedline
can add, replace or remove any of them through `Keybindings`; what follows are
the defaults.

## How to read this

Modifiers are written `Ctrl`, `Alt` and `Shift`. On terminals that do not send
`Alt`, press `Esc` first and then the key.

Some keys are a fallback chain: reedline tries each step and takes the first
that applies. The tables write those with "otherwise".

Vi and Helix add a modal layer that reads plain characters before any table.

## Common bindings

Four sets are shared between modes:

| Set | Emacs | Vi insert | Vi normal / visual | Helix insert | Helix normal / select |
| --- | --- | --- | --- | --- | --- |
| [Control](#control) | yes | yes | yes | yes | yes |
| [Navigation](#navigation) | yes | yes | yes | yes | yes, rebound in select |
| [Editing](#editing) | yes | yes | no | yes | no |
| [Selection](#selection) | yes | yes | yes | yes | yes |

A mode without the editing set rebinds `Backspace` and `Delete` itself. Modes
may also override single keys within a set; those are marked in the mode's own
tables.

### Control

| Key | Action |
| --- | --- |
| `Esc` | Dismiss an open menu and clear the selection, in modal modes also switch mode |
| `Ctrl-c` | Abort the current input |
| `Ctrl-d` | Delete the grapheme under the cursor, or signal end-of-file when the buffer is empty |
| `Ctrl-l` | Clear the screen, keeping the current input |
| `Ctrl-r` | Search the history |
| `Ctrl-o` | Open the buffer in the configured external editor |

### Navigation

| Key | Action |
| --- | --- |
| `Up`, `Ctrl-p` | Move up in an open menu, otherwise walk history backwards |
| `Down`, `Ctrl-n` | Move down in an open menu, otherwise walk history forwards |
| `Left` | Move left in an open menu, otherwise move one grapheme left |
| `Right` | Accept a history hint, otherwise move right in an open menu, otherwise move one grapheme right |
| `Ctrl-Left` | Move one word left |
| `Ctrl-Right` | Accept one word of a history hint, otherwise move one word right |
| `Home`, `Ctrl-a` | Move to the start of the line |
| `End`, `Ctrl-e` | Accept a history hint, otherwise move to the end of the line |
| `Ctrl-Home` | Move to the start of the buffer |
| `Ctrl-End` | Move to the end of the buffer |
| `Alt-<` | Move to the start of the buffer |
| `Alt->` | Move to the end of the buffer |

`Shift-Alt-,` and `Shift-Alt-.` are alternative spellings of `Alt-<` and
`Alt->` for terminals speaking the Kitty keyboard protocol.

### Editing

| Key | Action |
| --- | --- |
| `Backspace`, `Ctrl-h` | Delete the grapheme to the left |
| `Delete` | Delete the grapheme under the cursor |
| `Ctrl-Backspace`, `Ctrl-w` | Delete the word to the left |
| `Ctrl-Delete` | Delete the word to the right |
| `Shift-Enter`, `Alt-Enter` | Insert a newline without submitting |
| `Ctrl-j` | Submit the input, or insert a newline if it is incomplete |

None of these fill the cut buffer. Emacs rebinds `Ctrl-w` to a cut, so there it
does.

With the `system_clipboard` feature enabled, this set also carries:

| Key | Action |
| --- | --- |
| `Ctrl-Shift-x` | Cut the selection to the system clipboard |
| `Ctrl-Shift-c` | Copy the selection to the system clipboard |
| `Ctrl-Shift-v` | Paste from the system clipboard |

### Selection

| Key | Action |
| --- | --- |
| `Shift-Left`, `Shift-Right` | Extend the selection by one grapheme |
| `Shift-Up`, `Shift-Down` | Extend the selection by one line |
| `Ctrl-Shift-Left`, `Ctrl-Shift-Right` | Extend the selection by one word |
| `Shift-Home`, `Shift-End` | Extend the selection to the line start or end |
| `Ctrl-Shift-Home`, `Ctrl-Shift-End` | Extend the selection to the buffer start or end |
| `Ctrl-Shift-a` | Select the whole buffer |

## Emacs mode

Reedline's default when no edit mode is configured. All four
[common sets](#common-bindings), plus:

### Movement

| Key | Action |
| --- | --- |
| `Ctrl-b` | Move left in an open menu, otherwise move one grapheme left |
| `Ctrl-f` | Accept a history hint, otherwise move right in an open menu, otherwise move one grapheme right |
| `Alt-Left`, `Alt-b` | Move one word left |
| `Alt-Right`, `Alt-f` | Accept one word of a history hint, otherwise move one word right |

### Editing

| Key | Action |
| --- | --- |
| `Enter` | Submit the input, or insert a newline if it is incomplete |
| `Alt-Backspace`, `Alt-m` | Delete the word to the left |
| `Alt-Delete` | Delete the word to the right |
| `Ctrl-w` | Cut the word to the left, overriding the common binding |
| `Alt-d` | Cut the word to the right |
| `Ctrl-k` | Cut to the end of the line, or join the next line when already at the end |
| `Ctrl-u` | Cut from the start of the line to the cursor |
| `Ctrl-y` | Paste the cut buffer before the cursor |
| `Ctrl-t` | Swap the two graphemes around the cursor |
| `Ctrl-z` | Undo |
| `Ctrl-g` | Redo |
| `Alt-u` | Uppercase the whole word under the cursor |
| `Alt-l` | Lowercase the whole word under the cursor |
| `Alt-c` | Capitalize the grapheme under the cursor |

## Vi mode

An operator waits for a motion, and the motion picks the range it acts on:
`d` then `w` cuts a word.

Vi starts in insert mode. `Esc` switches to normal, `i` and its siblings return
to insert, and `v` enters visual from normal. `Esc` leaves visual for normal;
in normal it cancels a half-typed command. `v` only enters visual when nothing
is half-typed, so `rv` and `fv` take it as their character argument.

A count prefixes a motion or a command. Counts on an operator and its motion
multiply: `2d3w` cuts six words.

### Insert mode

All four [common sets](#common-bindings), plus:

| Key | Action |
| --- | --- |
| `Enter` | Submit the input, or insert a newline if it is incomplete |
| `Esc` | Switch to normal mode |

The cut and case commands are not bound here: `Ctrl-k`, `Ctrl-u`, `Ctrl-y`,
`Ctrl-t`, `Alt-d`, `Alt-u`, `Alt-l` and `Alt-c` belong to Emacs mode. `Ctrl-w`
deletes a word left without cutting it.

### Normal and visual mode

The control, navigation and selection sets, but not editing. In its place:

| Key | Action |
| --- | --- |
| `Backspace` | Move one grapheme left |
| `Delete` | Delete the grapheme under the cursor |

#### Motions

In visual mode a motion extends the selection instead of moving the cursor.

| Key | Action |
| --- | --- |
| `h`, `l` | Move one grapheme left or right |
| `j`, `k` | Move one line down or up, walking history at the buffer edge |
| `w`, `W` | Move to the start of the next word or WORD |
| `e`, `E` | Move to the end of the next word or WORD |
| `b`, `B` | Move to the start of the previous word or WORD |
| `0` | Move to the start of the line |
| `^` | Move to the first non-blank of the line |
| `$` | Move to the end of the line |
| `gg`, `G` | Move to the start or end of the buffer |
| `f<char>`, `F<char>` | Move onto the next or previous occurrence of `<char>` |
| `t<char>`, `T<char>` | Move just before the next or previous occurrence of `<char>` |
| `;`, `,` | Repeat the last `f`/`t`/`F`/`T` search, forwards or reversed |

A WORD is whitespace-delimited, a word is not.

#### Normal mode commands

| Key | Action |
| --- | --- |
| `i`, `a` | Insert before or after the cursor |
| `I`, `A` | Insert at the start or at the end of the line |
| `o`, `O` | Open a line below or above and insert |
| `v` | Enter visual mode |
| `x`, `X` | Cut the grapheme under or before the cursor |
| `s` | Cut the grapheme under the cursor and insert |
| `r<char>` | Replace the grapheme under the cursor with `<char>` |
| `C` | Change to the end of the line, without filling the cut buffer |
| `S` | Change the whole line |
| `D` | Cut to the end of the line |
| `p`, `P` | Paste the cut buffer after or before the cursor |
| `u` | Undo |
| `~` | Switch the case of the grapheme under the cursor |
| `.` | Repeat the last change |
| `?` | Search the history |
| `Enter` | Submit the input, or insert a newline and switch to insert if it is incomplete |

#### Operators

`d`, `c` and `y` take any motion and cut, change or copy the range it covers.
Doubling applies the operator to the whole line: `dd`, `cc`, `yy`.

An operator can take a text object instead, `i` for inside and `a` for around:

| Object | Selects |
| --- | --- |
| `w`, `W` | A word or WORD |
| `b` | The enclosing brackets |
| `q` | The enclosing quotes |

A delimiter works directly too: `(`, `)`, `[`, `]`, `{`, `}`, `<`, `>`, `"`,
`'`, `` ` `` and `$`. Either half of a pair does the same, so `di(` equals
`di)`. All three operators take the inside form, only `d` and `y` take the
around form.

#### Visual mode commands

| Key | Action |
| --- | --- |
| `d`, `x` | Cut the selection and return to normal mode |
| `X` | Cut the selected lines, staying in visual mode |
| `c`, `s` | Change the selection and switch to insert mode |
| `y` | Copy the selection and return to normal mode |
| `p`, `P` | Replace the selection with the cut buffer, after or before, staying in visual mode |
| `u`, `U` | Lowercase or uppercase the selection, then return to normal mode |
| `~` | Switch the case of the selection, then return to normal mode |
| `o`, `O` | Move the cursor to the other end of the selection |
| `r<char>` | Replace every selected grapheme with `<char>`, then return to normal mode |
| `Esc` | Drop the selection and return to normal mode |

## Helix mode

A motion carries a selection with it, and a verb acts on what is selected.
There is no operator-pending state: `w` selects a word, `d` then deletes it.

Helix starts in insert mode. `Esc` switches to normal, `i` and its siblings
return to insert, and `v` toggles select mode. In select mode a motion extends
the selection instead of replacing it.

A count prefixes any modal key. A pending count sends the next key straight to
the modal layer, bypassing the keybinding table, so `3Alt-d` is not `Alt-d`
three times.

Descriptions follow
[upstream's keymap](https://docs.helix-editor.com/keymap.html).

### Insert mode

All four [common sets](#common-bindings), plus:

| Key | Action |
| --- | --- |
| `Enter` | Submit the input, or insert a newline if it is incomplete |
| `Esc` | Switch to normal mode |

### Normal and select mode

The control, navigation and selection sets, but not editing. In its place:

| Key | Action |
| --- | --- |
| `Backspace` | Move one grapheme left in normal mode, extend one left in select mode |
| `Delete` | Delete the grapheme under the cursor |
| `Alt-d` | Delete selection, without yanking |
| ``Alt-` `` | Set the selected text to upper case |

Select mode rebinds the navigation set so each key extends the way its modal
twin does: the arrows follow `h`/`j`/`k`/`l`, `Ctrl-Left`/`Ctrl-Right` follow
`b`/`w`, `Home`/`End` and `Ctrl-a`/`Ctrl-e` follow `gh`/`gl`, and
`Ctrl-Home`/`Ctrl-End` with `Alt-<`/`Alt->` follow `gg`/`ge`. `Up` and `Down`
extend by line and never reach history, which would replace the buffer the
selection is anchored in.

#### Motions

| Key | Action |
| --- | --- |
| `h`, `l` | Move left / right |
| `j`, `k` | Move down / up |
| `w`, `b`, `e` | Move next word start / previous word start / next word end |
| `W`, `B`, `E` | Move next WORD start / previous WORD start / next WORD end |
| `f<char>`, `F<char>` | Find next char / find previous char |
| `t<char>`, `T<char>` | Find till next char / find till previous char |
| `gh`, `gl` | Go to the start of the line / go to the end of the line |
| `gs` | Go to first non-whitespace character of the line |
| `gg`, `ge` | Go to the start of the buffer / go to the end of the buffer |

`h`, `l` and the `g` motions collapse the selection onto the new position;
`w`, `b`, `e`, `f` and `t` select what they traverse. `j` and `k` walk history
at the buffer edge, since a prompt has nowhere above its first line to go.

#### Selection

| Key | Action |
| --- | --- |
| `x` | Select current line, if already selected, extend to next line |
| `%` | Select entire buffer |
| `v` | Toggle select mode |
| `Esc` | Collapse the selection in normal mode, leave select mode in select mode |

Upstream keeps the selection on `Esc` in normal mode; reedline collapses it,
since `;` is not available and this is otherwise the only way to drop one.

#### Changes

| Key | Action |
| --- | --- |
| `i`, `a` | Insert before selection / insert after selection (append) |
| `I`, `A` | Insert at the first non-blank of the line / insert at the end of the line |
| `o`, `O` | Open new line below selection / open new line above selection |
| `d` | Delete selection |
| `c` | Change selection (delete and enter insert mode) |
| `y` | Yank selection |
| `p`, `P` | Paste after selection / paste before selection |
| `r<char>` | Replace with a character |
| `~` | Switch case of the selected text |
| `` ` `` | Set the selected text to lower case |
| `u`, `U` | Undo change / redo change |
| `Enter` | Submit the input, or insert a newline and switch to insert if it is incomplete |

`d`, `y` and `p` return to normal mode, `c` switches to insert.

### Not yet available

`;`, `,`, `J`, `G`, `X`, `>`, `<`, the `/` `n` `N` search family, the `m` match
mode and the multi-cursor commands.
