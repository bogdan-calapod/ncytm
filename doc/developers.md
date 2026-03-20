# Development

> **Note**: ncytm is a fork of [ncspot](https://github.com/hrkfdn/ncspot). This documentation is being updated to reflect the YouTube Music backend.

## Prerequisites
- A working [Rust installation](https://www.rust-lang.org/tools/install)
- Python 3 (needed for building `rust-xcb` dependency)

On Linux, you also need:

- `pkgconf` (sometimes called `pkg-config`)
- Development headers for the aforementioned runtime dependencies:
  - Debian and derivatives:
    ```sh
    sudo apt install libdbus-1-dev libncursesw5-dev libpulse-dev libssl-dev libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev
    ```
  - Fedora:
    ```sh
    sudo dnf install dbus-devel libxcb-devel ncurses-devel openssl-devel pulseaudio-libs-devel
    ```
  - Arch and derivatives:
    ```sh
    # headers are included in the base packages
    sudo pacman -S dbus libpulse libxcb ncurses openssl
    ```

## Debugging
For simple debugging, you can pass a debug log filename:

```sh
cargo run -- -d debug.log
```

It can be difficult to debug a TUI application as it might not run well in an IDE terminal or the
terminal could be used by the text editor. It is however possible to run ncytm in its own process
and attach a debugger. On Linux this can be achieved with `gdb` or `lldb`. It is important that
[ptrace](https://www.kernel.org/doc/html/latest/admin-guide/LSM/Yama.html) is disabled for this to
work. To disable it, execute `echo 0 | sudo tee /proc/sys/kernel/yama/ptrace_scope`. This will allow
any process to inspect the memory of another process. It is automatically re-enabled after a reboot.

If ncytm has crashed you can find the latest backtrace at `$NCYTM_CACHE_DIRECTORY/backtrace.log`.
The cache directory can be shown by running `ncytm info`.

## Compiling

Clone and build locally:

```sh
git clone https://github.com/anomalyco/ncytm
cargo build --release
```

**You may need to manually set the audio backend on non-Linux OSes.** See [Audio
Backends](#audio-backends).

## Audio Backends
ncytm uses PulseAudio by default. Support for other backends can be enabled with the following
commands.

PortAudio for BSD's or macOS
```sh
cargo build --no-default-features --features portaudio_backend,pancurses_backend
```

Rodio for Windows
```sh
cargo build --no-default-features --features rodio_backend,pancurses_backend
```

## Other Features
Here are some auxiliary features you may wish to enable:

| Feature           | Default | Description                                                                                |
|-------------------|---------|--------------------------------------------------------------------------------------------|
| `cover`           | off     | Add a screen to show the album art.                                                        |
| `mpris`           | on      | Control `ncytm` via dbus. See [Arch Wiki: MPRIS](https://wiki.archlinux.org/title/MPRIS). |
| `notify`          | on      | Send a notification to show what's playing.                                                |
| `share_clipboard` | on      | Ability to copy the URL of a song/playlist/etc. to system clipboard.                       |

Consult [Cargo.toml](/Cargo.toml) for the full list of supported features.
