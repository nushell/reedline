# UX Test Checklist

As we currently don't have automated tests for the user facing terminal logic, we still have to check a few things manually.
This list does not try to cover every case but tries to catch the most likely breaking points or previous gotchas.
Exhaustiveness should be achieved by covering the components with appropriate unit tests.

## Do I have to perform all the manual tests?

Ideally we would validate the user experience for every PR but there are probably some good heuristics for when it is a *really good* idea to run through the manual checklist.

- Your PR changed the repaint logic.
- You changed how key presses are dispatched.
- You added a completely new component.
- The component you changed is not covered by tests, that uphold a contract for the I/O facing engine.
- You did a large refactoring touching several components at once.

## Configuration

To catch potential index overflows etc. running the example binary in debug mode via `cargo run` can be helpful. Yet in some cases the experience might be better/smoother when running the actual release build via `cargo run --release`. This is especially true for resizing. If the slower execution in debug mode causes noticeable issues report them with the checklist.

> Copy the checklist below, as part of your PR finalization

## Manual checks

Relevant features tested (leave open if you did not consider those areas touched by your PR):

- [ ] core editing and default Emacs keybindings
- [ ] history
- [ ] syntax highlighting
- [ ] completion/hinting
- [ ] vi mode

### Info

Build: [ ] debug / [ ] release

Platform:

Terminal emulator:

Inside a [ ] ssh,[ ] tmux or [ ] screen session?

### Basics

- [ ] Typing of a short line containing both upper- and lowercase characters.
- [ ] Movement left/right using the arrow keys
- [ ] Word to the left with `Ctrl-b` or `Ctrl-Left`, Word to the right with `Ctrl-f`
- [ ] `Enter` to complete entry

#### Clearing

- [ ] Type something and abort the entry with `Ctrl-c`, you should end up on an empty prompt below.
- [ ] Type something and press `Ctrl-l` to clear the screen. Your current entry should still be there and passed through when pressing `Enter`

#### Unicode and Emojis

- [ ] Paste the line `Emoji test 😊 checks 🤦🏼‍♂️ unicode` and move the cursor over the emojis.
- [ ] Are you able to delete the smiley?
- [ ] `Home`/`End` at accurate positions
- [ ] Check that the emoji containing line can be entered

## History

- [ ] On the empty line press the `up-arrow` key to see if you can recall the previous entry
- [ ] Press `Enter` to execute this line (it should *not* be duplicated in the history, after checking leave history recall by `down-arrow`)
- [ ] On an empty line start typing the beginning of a line in the history. Hit the `up-arrow` to find the matching entry.
- [ ] After that run `Ctrl-r` to start traditional reverse search. Type your initial search. Can you find more hits by pressing `Ctrl-r` or `up-arrow`?
- [ ] Abort this search by pressing `Ctrl-c`

## Syntax highlighting

- [ ] Upon entering `test`, this word is highlighted differently.

## Completion / hinting

Use an example that wires a completer, menu, and Tab binding (defaults do not
bind Tab by themselves). Prefer `cargo run --example demo` (Emacs) which also
enables `with_quick_completions(true)` and `with_partial_completions(true)`, or
`cargo run --example completions` for a smaller columnar menu only. For
fish-style hints alone, `cargo run --example hinter` works after you have
entered a few history lines.

### Completion menu

- [ ] Type a prefix that matches several entries (e.g. `he` or `aba` in the
      demo). Press `Tab`: a completion menu opens with matching suggestions.
- [ ] Press `Tab` again (or the arrow keys) to move the selection through the
      menu. `Shift-Tab` moves backward when that binding is present (demo).
- [ ] With a suggestion highlighted, press `Enter`: the selection is inserted
      into the line buffer and the menu closes. The line is *not* submitted.
- [ ] Open the menu again and press `Esc`: the menu dismisses without changing
      the buffer beyond any partial fill already applied.
- [ ] Narrow the prefix until only one suggestion remains (demo with quick
      completions): that value is accepted automatically without needing
      `Enter`.
- [ ] With several suggestions that share a longer prefix (demo with partial
      completions), `Tab` / menu navigation fills the common prefix in the
      buffer before or while cycling options.
- [ ] Type a prefix with no matches and press `Tab`: no bogus insertion; the
      buffer stays as typed (menu stays empty / inactive as applicable).

### History hints (`DefaultHinter`)

Requires a hinter (demo or `cargo run --example hinter`) and prior history
entries that share a prefix with what you type.

- [ ] After recalling or entering a longer line, on a new prompt type only its
      beginning: the remainder appears as a dimmed inline hint.
- [ ] Press `Right` or `End` (or Emacs `Ctrl-f` / `Ctrl-e` when those bindings
      apply): the full hint is accepted into the buffer.
- [ ] With a multi-word hint visible, press `Ctrl-Right` (or Emacs `Alt-f`):
      only the next hint word is accepted, not the entire remainder.
- [ ] With an open completion menu, arrow keys prefer the menu over hint
      accept / cursor motion (fallback chain in the default navigation set).

## VI mode

Run `cargo run --example demo -- --vi` (or construct `Vi` with the default
insert/normal keybinding sets). Reedline Vi starts in **insert** mode. Full
tables live in `KEYBINDINGS.md`; this checklist covers the gotchas most likely
to break during refactors.

### Mode transitions

- [ ] Type a few characters in insert mode, then press `Esc`: you enter normal
      mode and the cursor steps back onto the last typed grapheme.
- [ ] From normal mode, `i` inserts before the cursor; `a` inserts after it;
      `I` / `A` jump to line start / end and insert.
- [ ] From normal mode, `v` enters visual mode; `Esc` returns to normal and
      clears the selection.
- [ ] In normal mode, a half-typed command (e.g. bare `d` or `f`) is cancelled
      by `Esc` without leaving normal mode.

### Motions and edits (normal mode)

- [ ] `h` / `l` move one grapheme left / right; `j` / `k` move by line and, at
      the buffer edge, walk history like the arrow keys.
- [ ] `w` / `b` / `e` move by word; `0` / `$` move to line start / end.
- [ ] `x` cuts the grapheme under the cursor; `u` undoes.
- [ ] Operator + motion: `dw` cuts a word; `dd` cuts the line; `yy` yanks the
      line; `p` / `P` paste after / before the cursor.
- [ ] `r<char>` replaces the grapheme under the cursor; `.` repeats the last
      change when one exists.

### Visual mode and shared controls

- [ ] In visual mode, `hjkl` (and word motions) extend the selection; `d` /
      `x` cut it and return to normal; `y` yanks and returns to normal; `c`
      changes (cut + insert).
- [ ] Common controls still work from insert and normal: `Ctrl-c` aborts,
      `Ctrl-l` clears the screen keeping the buffer, `Ctrl-r` opens history
      search. In normal mode `?` also starts history search and switches to
      insert.
- [ ] From insert, an unbound `Alt-<char>` is treated as `Esc` then `<char>`
      (readline/zsh meta convention), e.g. `Alt-k` leaves insert and recalls
      the previous history line—unless that Alt chord is explicitly bound.

## Unit-test friction (optional follow-ups)

Behaviors that are easy to regress in I/O-facing code but already have (or
clearly deserve) engine/unit coverage rather than only this checklist:

- Menu accept vs submit: `Enter` must `MenuAccept` (replace + deactivate) when
  a menu has fresh suggestions, and only submit when the menu declines.
- Quick / partial completion decisions must agree between menu activation and
  deferred/late completer results (stale suggestions must not auto-accept).
- Vi leaving-insert cursor step-back must compose with menu/`Esc` deactivation
  and with the Alt-as-meta dispatch path.
- History-hint accept (`HistoryHintComplete` / `HistoryHintWordComplete`) vs
  open-menu arrow fallbacks in the shared navigation bindings.
