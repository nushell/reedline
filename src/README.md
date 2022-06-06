# Reedline internal developer documentation

**Attention** The README files in the source folders are primarily intended to document the current implementation details and quirks as well as requirements we currently strive towards.
It is not intended as the public documentation for library users!
While we try to take care that the primary documentation also shared on <https://docs.rs/reedline> stays current with the behavior of the implementation and is doctested accordingly.
The internal documents may get out of sync more easily and require good will by contributors to stay up to date if systems are refactored.

## Design requirements

Reedline is currently developed as the bundled line editor for [nushell](https://github.com/nushell/nushell) but tries to remain agnostic of the actual language implementation so it can easily be used for other projects.
As the feedback loop for nushell project contributors is typically the shortest, we might implicitly favour optimizations or changes driven by nushell's requirements.

### Core requirements

With general functionality:

- Support terminals on all major platforms: Windows, macOS, Linux
- Support a variety of terminal emulators: e.g. Terminal.app on macOS, iterm2, Windows terminal, gnome-terminal (and those using its vte backend), xterm derivatives, alacritty, kitty, wezterm, and a bunch more
- Be usable despite the fact that the different platforms and terminal emulators might have restricted support for certain functionalities (core ANSI terminal support or extensions to that) or have some key events mapped to core system functions.
- Have integrations for syntax highlighting, tab completions etc. that are implemented by the using programming language/environment to provide modern comforts.
- have configurable prompts, a history, and some other goodies
- Make the keybindings configurable to some extent
- Support sufficient configuration to allow customization
- Be aware of unicode characters and display them as good as the terminal can. (Currently we only have thought of left to right text flow!)

## Goals

- don't `panic!`: as a library we should strive towards a behavior where any panic should reflect a serious bug on our side and errors that result from the reality of a system are reflected as useful result types.
- the most important thing to keep correct: Have consistent display of the currently entered line. The displayed and submitted version should not deviate!
- be a nice citizen: Don't cause display artifacts on the current screen, avoid overwriting previous output and if possible maintain consistent display in the scroll-back buffer. Hard to reach goal: handle resizes of the terminal window gracefully.
- Our defaults should not be surprising, only pleasantly surprising

## Non-goals

- General terminal input functionality beyond the use as a programming language REPL or general command box: (for example not as a password prompt or a prompt that only accepts entries in a certain format e.g. number or date input box)
- maximizing the compatibility/similarity with an existing (line) editor at the cost of flexibility to support workflows from other editors.
- be a standalone text editor
- be useful outside of rust as a shared library with a maintained ABI

## Technical background

- [ANSI terminal](https://en.wikipedia.org/wiki/ANSI_escape_code): standardized protocol to have control and styling sequences "in-band" with the text. That means on Unix systems most of the terminal control is written to the same buffer as the output and special input events are also encoded with the content that represents user input. The core of this has been standardized many moons ago <https://www.ecma-international.org/publications-and-standards/standards/ecma-48/> but has been extended with additional control sequences by various physical terminals (like the [VT-series of terminals](https://vt100.net/docs/vt510-rm/contents.html), some of it accepted spec) and also by terminal emulators (like [xterm](https://invisible-island.net/xterm/xterm.html)).
  - In a regular mode the terminal will display user typed input, to respond to it we enable something called raw mode (changing some settings depending on the platform) to listen to the events and handle them ourselves
  - Some control is directly handled by the pure ASCII characters (they contain 32 control sequences in the lowest bit values), this has some implications: in raw mode `\n` or `LF` will only move a line down not as expected on unix also return to the beginning of the next line, thus you have to send like on windows `\r\n` `CRLF` for drawing operations with enabled raw mode. Also some keybindings with `Ctrl` are unusable as `CTRL-<key>` can be encoded in shifted bits for some characters like `tab`
- On Windows NT the terminal configuration is managed primarily out of band with calls to Windows functions using a handle to the terminal. Details on that can be found [here](https://docs.microsoft.com/en-us/windows/console/) but we luckily can abstract most of that with `crossterm`.

## Current design decisions

- To handle sending styling commands and receiving events we currently use the excellent [crossterm crate](https://github.com/crossterm-rs/crossterm). It not only encodes a variety of ANSI sequences for styling or terminal setup but also hooks into Windows APIs for the non ANSI compliant functions of the Windows terminal.
