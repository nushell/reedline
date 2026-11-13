# Contributing

Feel free to contribute, whether you use reedline through Nushell or build your own shell or REPL on it.

## Before you start

- **A small fix or a documentation change:** open a pull request directly.
- **Anything larger**, such as a new feature, a change to the public API or a change in default behaviour: talk to us first. Use the `#reedline` channel of the [Nushell Discord](https://discord.gg/NtAbbGn), or comment on the roadmap issue your idea belongs to:
  - [Prompt, history, hints and the API](https://github.com/nushell/reedline/issues/1284)
  - [Edit modes: emacs, vi and helix](https://github.com/nushell/reedline/issues/1221)
  - [Completion menu: actions, insertion and display](https://github.com/nushell/reedline/issues/1283)
- **A bug or a missing feature you don't plan to work on:** open an [issue](https://github.com/nushell/reedline/issues/new/choose).

## Set up

Reedline builds with stable Rust and has no setup beyond cloning.

```sh
git clone https://github.com/nushell/reedline
cd reedline
cargo run --example demo
```

The tests run with [cargo-nextest](https://nexte.st/docs/installation/pre-built-binaries/).

## Checks

CI runs the following on every pull request and treats warnings as errors.
Run them locally before you push:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all -- -D warnings
cargo nextest run --all
cargo test --doc
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
```

Things that are easy to miss:

- **Feature sets.** CI repeats these commands for each of `bashisms`, `sqlite`, `bashisms,sqlite` and `system_clipboard`. If your change touches code behind a feature, run them again with `--features <name>`.
- **Doctests.** nextest does not run them, which is why `cargo test --doc` is a separate step.
- **Doc comments.** Every public item needs one, and a missing one fails the clippy step.
- **Lockfile.** `Cargo.lock` has to match `Cargo.toml`; `cargo check --locked` tells you.
- **Minimum Rust version.** The library has to build on the `rust-version` in `Cargo.toml`, so avoid newer language and standard library features.
- **Spelling.** A [typos](https://github.com/crate-ci/typos) check runs on the whole repository.

## Testing by hand

Automated tests do not cover everything a terminal does.
Go through the checklist in [UX_TESTING.md](./UX_TESTING.md) and paste it into your pull request when your change:

- touches how the prompt and buffer are painted,
- changes how key presses are dispatched,
- adds a new component, or
- refactors several components at once.

## Pull requests

- **Fill in the template.** The summary should say why the change is needed and whether it changes the public API or observable behaviour. Before and after descriptions help a lot.
- **One change per pull request.** Unrelated fixes are easier to review and to revert when they are separate.
- **Title.** Pull requests are squash-merged, so the title becomes the commit subject. We write it as `type(scope): summary`, for example `fix(menu): let a later menu replace one of the same name`, with a `!` after the scope for a breaking change.
- **Keybindings.** If you change a default binding, update [KEYBINDINGS.md](./KEYBINDINGS.md) to match.

## Finding your way around

- [`src/README.md`](./src/README.md) holds the internal design notes: requirements, goals and non-goals, technical background and the current design decisions.
- [`examples/`](./examples) has a small runnable program for each feature. They are the quickest way to exercise your change.
- [docs.rs/reedline](https://docs.rs/reedline) is the public API documentation, built from the doc comments.

## How it started

Reedline began as a series of [live-coding streams](https://www.twitch.tv/jntrnr) by [Sophia J. Turner](https://github.com/sophiajt).
