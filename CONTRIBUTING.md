# Contributing

Small, quiet, and tested. If a change makes the browser louder, slower, or
hungrier without earning it, it will be argued about.

## Before a pull request

```sh
cargo build --release
cargo test
python3 tools/stress.py --quick
tools/shots.sh council/latest      # and look at the pictures
```

If a picture changed, say so in the pull request and include it.

## What is welcome

- A bug with a reproduction: a page, a keystroke, a report entry.
- A shortcut that a person would expect and that is missing.
- A test that reproduces something broken.
- A smaller, plainer way to do something already done.

## What is not

- Dependencies. This browser has eight and every one of them is a cost.
- Refactors without a behaviour or a bug behind them.
- Any change that needs a design decision bigger than the diff: open an issue
  first.

## Style

One idea per module, one name per idea, comments that explain why rather than
what, and no debug output left behind. `cargo fmt` and `cargo clippy` before you
ask someone to read it.
