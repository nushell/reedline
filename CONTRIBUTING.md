# Contributing

reedline's development is primarily driven by the [nushell project](https://github.com/nushell) at the moment to provide its interactive REPL.
Our goal is to explore options for a pleasant interaction with a shell and programming language.
While the maintainers might currently prioritize working on features for nushell, we are open to ideas and contributions by people and projects interested in using reedline for other projects.
Feel free to open an [issue](https://github.com/nushell/reedline/issues/new/choose) or chat with us on the [nushell discord](https://discordapp.com/invite/NtAbbGn) in the dedicated `#reedline` channel

## Good starting points

If you want to get started, check out the list of [issues](https://github.com/nushell/reedline/issues) with the ["good first issue" label](https://github.com/nushell/reedline/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22).

If you want to follow along with the history of how reedline got started, you can watch the [recordings](https://youtube.com/playlist?list=PLP2yfE2-FXdQw0I6O4YdIX_mzBeF5TDdv) of [JT](https://github.com/jntrnr)`s [live-coding streams](https://www.twitch.tv/jntrnr).

[Playlist: Creating a line editor in Rust](https://youtube.com/playlist?list=PLP2yfE2-FXdQw0I6O4YdIX_mzBeF5TDdv)

## Developing

### Set up

This is no different than other Rust projects.

```bash
git clone https://github.com/nushell/reedline
cd reedline
# To try our example program
cargo run --example basic
```

### Code style

We follow the standard rust formatting style and conventions suggested by [clippy](https://github.com/rust-lang/rust-clippy).

### To make the CI gods happy

Before submitting a PR make sure to run:

- for formatting

  ```shell
  cargo fmt --all
  ```

- the clippy lints

  ```shell
  cargo clippy
  ```

- the test suite

  ```shell
  cargo test
  ```
