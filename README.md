# Monkeytype but as a native TUI application

## Features
- doesnt use a browser
- doesnt need a display server
- honestly i wouldnt say its anywhere near performant
- has a look
- probably has a 10MB binary

## Attributions
- [Miodec](https://github.com/Miodec) for creating, developing, and hosting [monkeytype](https://monkeytype.com)
- [ratatui](https://ratatui.rs) library for making this way easier to make

## Bugs
Dont report it to the monkeytype repo, report it here instead.

## Warnings
You may get banned by anticheat while using this. You have my warnings, and I hold no liability if that happens.

Using an account with leaderboard opted out with this application is recommended

## Building
### Linux
0. Install `cargo` if you don't have it:
```bash
curl https://sh.rustup.rs -sSf | sh
```
1. Install required dependencies:
```bash
sudo apt install build-essential pkg-config libssl-dev
```
2. Build program
```bash
cargo build --release
```

The resulting binary will be located at `target/release/typing`. You can move it to `$HOMEDIR/.local/bin/` to run anywhere:
```bash
mv target/release/typing $HOMEDIR/.local/bin/
```

### Windows
Good luck you're on your own

### BSDs
You can figure this out yourself
