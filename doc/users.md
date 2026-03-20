# User Documentation

> **Note**: ncytm is a fork of [ncspot](https://github.com/hrkfdn/ncspot). This documentation is being updated to reflect the YouTube Music backend. Many features are still in development.

## Installation Instructions

*Coming soon - ncytm is not yet packaged for distribution.*

For now, build from source:

```sh
git clone https://github.com/anomalyco/ncytm
cd ncytm
cargo build --release
```

### Runtime Dependencies (Linux)
- `dbus`, `libncurses`, `libssl`
- `libpulse` (or `portaudio`, if built using the PortAudio backend)
- `libxcb` (if built with the `clipboard` feature)
- `ueberzug` or a compatible implementation (e.g. `ueberzugpp`) (if built with the `cover` feature)

## Key Bindings
The keybindings listed below are configured by default. Additionally, if you
built `ncytm` with MPRIS support, you may be able to use media keys to control
playback depending on your desktop environment settings. Have a look at the
[configuration section](#configuration) if you want to set custom bindings.

### Navigation
| Key               | Command                                                                       |
|-------------------|-------------------------------------------------------------------------------|
| <kbd>?</kbd>      | Show help screen.                                                             |
| <kbd>F1</kbd>     | Queue (See [specific commands](#queue)).                                      |
| <kbd>F2</kbd>     | Search.                                                                       |
| <kbd>F3</kbd>     | Library (See [specific commands](#library)).                                  |
| <kbd>F8</kbd>     | Album Art (if built with the `cover` feature).                                |
| <kbd>/</kbd>      | Open a Vim-like search bar (See [specific commands](#vim-like-search-bar)).   |
| <kbd>:</kbd>      | Open a Vim-like command prompt (See [specific commands](#vim-like-commands)). |
| <kbd>Escape</kbd> | Close Vim-like search bar or command prompt.                                  |
| <kbd>Q</kbd>      | Quit `ncytm`.                                                                |
| <kbd>g</kbd>      | Go to the top of the current view (Vim motion).                               |
| <kbd>G</kbd>      | Go to the bottom of the current view (Vim motion).                            |

### Playback
| Key                           | Command                                                        |
|-------------------------------|----------------------------------------------------------------|
| <kbd>Return</kbd>             | Play track or playlist.                                        |
| <kbd>Space</kbd>              | Queue track or playlist.                                       |
| <kbd>.</kbd>                  | Play the selected item after the currently playing track.      |
| <kbd>P</kbd>                  | Move to the currently playing track in the queue.              |
| <kbd>S</kbd>                  | Save the currently playing item to your library.               |
| <kbd>D</kbd>                  | Remove the currently playing item from your library.           |
| <kbd>Shift</kbd>+<kbd>P</kbd> | Toggle playback (i.e. Play/Pause).                             |
| <kbd>Shift</kbd>+<kbd>S</kbd> | Stop playback.                                                 |
| <kbd>Shift</kbd>+<kbd>U</kbd> | Update the library cache (tracks, artists, albums, playlists). |
| <kbd><</kbd>                  | Play the previous track.                                       |
| <kbd>></kbd>                  | Play the next track.                                           |
| <kbd>F</kbd>                  | Seek forward by 1 second.                                      |
| <kbd>Shift</kbd>+<kbd>F</kbd> | Seek forward by 10 seconds.                                    |
| <kbd>B</kbd>                  | Seek backward by 1 second.                                     |
| <kbd>Shift</kbd>+<kbd>B</kbd> | Seek backward by 10 seconds.                                   |
| <kbd>-</kbd>                  | Decrease volume by 1%.                                         |
| <kbd>+</kbd>                  | Increase volume by 1%.                                         |
| <kbd>[</kbd>                  | Decrease volume by 5%.                                         |
| <kbd>]</kbd>                  | Increase volume by 5%.                                         |
| <kbd>R</kbd>                  | Toggle _Repeat_ mode.                                          |
| <kbd>Z</kbd>                  | Toggle _Shuffle_ state.                                        |

### Context Menus
| Key                           | Command                                                                                                   |
|-------------------------------|-----------------------------------------------------------------------------------------------------------|
| <kbd>O</kbd>                  | Open a detail view or context for the **selected item**.                                                  |
| <kbd>Shift</kbd>+<kbd>O</kbd> | Open a context menu for the **currently playing track**.                                                  |
| <kbd>A</kbd>                  | Open the **album view** for the selected item.                                                            |
| <kbd>Shift</kbd>+<kbd>A</kbd> | Open the **artist view** for the selected item.                                                           |
| <kbd>M</kbd>                  | Open the **recommendations view** for the **selected item**.                                              |
| <kbd>Shift</kbd>+<kbd>M</kbd> | Open the **recommendations view** for the **currently playing track**.                                    |
| <kbd>Ctrl</kbd>+<kbd>V</kbd>  | Open the context menu for a YouTube Music link in your clipboard (if built with the `share_clipboard` feature). |
| <kbd>Backspace</kbd>          | Close the current view.                                                                                   |

When pressing <kbd>O</kbd>:

- If the _selected item_ is **not** a track, it opens a detail view.
- If the _selected item_ **is** a track, it opens a context menu with:
  - "Artist(s)" (let's you show or (un)follow a track's artist(s))
  - "Show Album"
  - "Share" (if built with the `share_clipboard` feature)
  - "Add to playlist"
  - "Similar tracks"

### Sharing
(if built with the `share_clipboard` feature)

| Key                           | Command                                                                  |
|-------------------------------|--------------------------------------------------------------------------|
| <kbd>X</kbd>                  | Copy the URL to the **currently selected item** to the system clipboard. |
| <kbd>Shift</kbd>+<kbd>X</kbd> | Copy the URL to the **currently playing track** to the system clipboard. |

### Queue
| Key                          | Command                              |
|------------------------------|--------------------------------------|
| <kbd>C</kbd>                 | Clear the entire queue.              |
| <kbd>D</kbd>                 | Delete the currently selected track. |
| <kbd>Ctrl</kbd>+<kbd>S</kbd> | Save the current queue.              |

### Library
| Key          | Command                                 |
|--------------|-----------------------------------------|
| <kbd>D</kbd> | Delete the currently selected playlist. |

### Vim-Like Search Bar
| Key          | Command                     |
|--------------|-----------------------------|
| <kbd>n</kbd> | Previous search occurrence. |
| <kbd>N</kbd> | Next search occurrence.     |

### Vim-Like Commands
You can open a Vim-style command prompt using <kbd>:</kbd>, and close it at any
time with <kbd>Escape</kbd>.

The following is an abridged list of the more useful commands. For the full list, see [source code](/src/command.rs).

Note: \<FOO\> - mandatory arg; [BAR] - optional arg

| Command                                                          | Action                                                                                                                                                                                                                                                          |
|------------------------------------------------------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `help`                                                           | Show current key bindings.                                                                                                                                                                                                                                      |
| `quit`<br/>Aliases: `q`, `x`                                     | Quit `ncytm`.                                                                                                                                                                                                                                                  |
| `logout`                                                         | Remove any cached credentials from disk and quit `ncytm`.                                                                                                                                                                                                      |
| `playpause`<br/>Aliases: `pause`, `toggleplay`, `toggleplayback` | Toggle playback.                                                                                                                                                                                                                                                |
| `stop`                                                           | Stop playback.                                                                                                                                                                                                                                                  |
| `seek` [`+`\|`-`]\<TIME\>                                        | Seek to the specified position, or seek relative to current position by prepending `+`/`-`.<br/>\* TIME is anything accepted by [parse_duration](https://docs.rs/parse_duration/latest/parse_duration/)<br/>\* Default unit is `ms` for backward compatibility. |
| `move` \<DIRECTION\> \<STEP_SIZE\>                               | Scroll the current view `up`/`down`/`left`/`right` with integer step sizes, or `pageup`/`pagedown`/`pageleft`/`pageright` with float step sizes.                                                                                                                |
| `repeat` [REPEAT_MODE]<br/>Alias: `loop`                         | Set repeat mode. Omit argument to step through the available modes.<br/>\* Valid values for REPEAT_MODE: `list` (aliases: `playlist`, `queue`), `track` (aliases: `once`, `single`), `none` (alias: `off`)                                                      |
| `shuffle` [`on`\|`off`]                                          | Enable or disable shuffle. Omit argument to toggle.                                                                                                                                                                                                             |
| `previous`                                                       | Play the previous track.                                                                                                                                                                                                                                        |
| `next`                                                           | Play the next track.                                                                                                                                                                                                                                            |
| `focus` \<SCREEN\>                                               | Switch to a different view.<br/>\* Valid values for SCREEN: `queue`, `search`, `library`, `cover` (if built with the `cover` feature)                                                                                                                           |
| `search` \<SEARCH\>                                              | Search for a song/artist/album/etc.                                                                                                                                                                                                                             |
| `clear`                                                          | Clear the queue.                                                                                                                                                                                                                                                |
| `share` \<ITEM\>                                                 | Copy a shareable URL of the item to the system clipboard. Requires the `share_clipboard` feature.<br/>\* Valid values for ITEM: `selected`, `current`                                                                                                           |
| `newplaylist` \<NAME\>                                           | Create a new playlist.                                                                                                                                                                                                                                          |
| `sort` \<SORT_KEY\> [SORT_DIRECTION]                             | Sort a playlist.<br/>\* Valid values for SORT_KEY: `title`, `album`, `artist`, `duration`, `added`<br/>\* Valid values for SORT_DIRECTION: `ascending` (default; aliases: `a`, `asc`), `descending` (aliases: `d`, `desc`)                                      |
| `exec` \<CMD\>                                                   | Execute a command in the system shell.<br/>\* Command output is printed to the terminal, so redirection (`2> /dev/null`) may be necessary.                                                                                                                      |
| `noop`                                                           | Do nothing. Useful for disabling default keybindings. See [custom keybindings](#custom-keybindings).                                                                                                                                                            |
| `reload`                                                         | Reload the configuration from disk. See [Configuration](#configuration).                                                                                                                                                                                        |
| `reconnect`                                                      | Reconnect to YouTube Music (useful when session has expired or connection was lost)                                                                                                                                                                             |
| `add [current]`                                                  | Add selected track to playlist, if `current` is passed the currently playing track will be added                                                                                                                                                                |
| `save [current]`                                                 | Save selected item, if `current` is passed the currently playing item will be saved                                                                                                                                                                             |

## Remote control (IPC)
Apart from MPRIS, ncytm will also create a domain socket on UNIX platforms (Linux, macOS, *BSD).
The socket will be created in the platform's runtime directory. Run `ncytm info` to show the
location of this directory on your platform. Applications or scripts can connect to this socket to
send commands or be notified of the currently playing track, i.e. with `netcat`:

```
% nc -U $NCYTM_CACHE_DIRECTORY/ncytm.sock
play
{"mode":{"Playing":...},"playable":{...}}
```

Each time the playback status changes (i.e. after sending the `play`/`playpause`
command or simply by playing the queue), the current status will be published as
a JSON structure.

Possible use cases for this could be:
- Controlling a detached ncytm session (in `tmux` for example)
- Displaying the currently playing track in your favorite application/status bar
- Setting up routines, i.e. to play specific songs/playlists when ncytm starts

### Extracting info on currently playing song
Using `netcat` and the domain socket, you can query the currently playing track
and other relevant information. Note that not all `netcat` versions are suitable,
as they typically tend to keep the connection to the socket open. OpenBSD's
`netcat` offers a work-around: by using the `-W` flag, it will close after a
specific number of packets have been received.

```
% nc -W 1 -U $NCYTM_CACHE_DIRECTORY/ncytm.sock
```

This results in a single output in `JSON` format, which can e.g. be parsed using [jq](https://stedolan.github.io/jq/).

## Configuration
Configuration is saved to the `config.toml` file in the platform's standard configuration directory.
Run `ncytm info` to show the location of this directory on your platform. To reload the
configuration during runtime use the `reload` command.

Possible configuration values are:

| Name                            | Description                                                    | Possible values                                                                       | Default             |
|---------------------------------|----------------------------------------------------------------|---------------------------------------------------------------------------------------|---------------------|
| `command_key`                   | Key to open command line                                       | Single character                                                                      | `:`                 |
| `initial_screen`                | Screen to show after startup                                   | `"library"`, `"search"`, `"queue"`, `"cover"`<sup>[1]</sup>                           | `"library"`         |
| `use_nerdfont`                  | Turn nerdfont glyphs on/off                                    | `true`, `false`                                                                       | `false`             |
| `flip_status_indicators`        | Reverse play/pause icon meaning<sup>[2]</sup>                  | `true`, `false`                                                                       | `false`             |
| `backend`                       | Audio backend to use                                           | String<sup>[3]</sup>                                                                  |                     |
| `backend_device`                | Audio device to configure the backend                          | String                                                                                |                     |
| `audio_cache`                   | Enable caching of audio files                                  | `true`, `false`                                                                       | `true`              |
| `audio_cache_size`              | Maximum size of audio cache in MiB                             | Number                                                                                |                     |
| `volnorm`                       | Enable volume normalization                                    | `true`, `false`                                                                       | `false`             |
| `volnorm_pregain`               | Normalization pregain to apply in dB (if enabled)              | Number                                                                                | `0.0`               |
| `default_keybindings`           | Enable default keybindings                                     | `true`, `false`                                                                       | `false`             |
| `notify`<sup>[4]</sup>          | Enable desktop notifications                                   | `true`, `false`                                                                       | `false`             |
| `gapless`                       | Enable gapless playback                                        | `true`, `false`                                                                       | `true`              |
| `shuffle`                       | Set default shuffle state                                      | `true`, `false`                                                                       | `false`             |
| `repeat`                        | Set default repeat mode                                        | `"off"`, `"track"`, `"playlist"`                                                      | `"off"`             |
| `playback_state`                | Set default playback state                                     | `"Stopped"`, `"Paused"`, `"Playing"`, `"Default"`                                     | `"Paused"`          |
| `library_tabs`                  | Tabs to show in library screen                                 | Array of `"tracks"`, `"albums"`, `"artists"`, `"playlists"`, `"browse"`               | All tabs            |
| `cover_max_scale`<sup>[1]</sup> | Set maximum scaling ratio for cover art                        | Number                                                                                | `1.0`               |
| `hide_display_names`            | Hides usernames in the library header and on playlists         | `true`, `false`                                                                       | `false`             |
| `statusbar_format`              | Formatting for tracks in the statusbar                         | See [track_formatting](#track-formatting)                                             | `%artists - %track` |
| `[track_format]`                | Set active fields shown in Library/Queue views                 | See [track formatting](#track-formatting)                                             |                     |
| `[notification_format]`         | Set the text displayed in notifications<sup>[4]</sup>          | See [notification formatting](#notification-formatting)                               |                     |
| `[theme]`                       | Custom theme                                                   | See [custom theme](#theming)                                                          |                     |
| `[keybindings]`                 | Custom keybindings                                             | See [custom keybindings](#custom-keybindings)                                         |                     |

1. If built with the `cover` feature.
2. By default the statusbar will show a play icon when a track is playing and
   a pause icon when playback is stopped. If this setting is enabled, the behavior
   is reversed.
3. Run `ncytm -h` for a list of devices.
4. If built with the `notify` feature.

### Custom Keybindings
Keybindings can be configured in `[keybindings]` section in `config.toml`.

Each key-value pair specifies one keybinding, where the key is a string in the
format of:

```
[MODIFIER+]<CHAR|NAMED_KEY>
where:
  MODIFIER: Shift|Alt|Ctrl
  CHAR: Any printable character
  NAMED_KEY: Enter|Space|Tab|Backspace|Esc|Left|Right|Up|Down
    |Ins|Del|Home|End|PageUp|PageDown|PauseBreak|NumpadCenter
    |F0|F1|F2|F3|F4|F5|F6|F7|F8|F9|F10|F11|F12
```

For implementation see [commands::CommandManager::parse_key](/src/commands.rs).

Its value is a string that can be parsed as a command. See
[Vim-Like Commands](#vim-like-commands).

<details>
  <summary>Examples: (Click to show/hide)</summary>

```toml
[keybindings]
# Bind "Shift+i" to "Seek forward 10 seconds"
"Shift+i" = "seek +10s"
```

To disable a default keybinding, set its command to `noop`:

```toml
# Use "Shift+q" to quit instead of the default "q"
[keybindings]
"Shift+q" = "quit"
"q" = "noop"
```

</details>

### Proxy
`ncytm` will respect system proxy settings defined via the `http_proxy`
environment variable.

```sh
# In sh-like shells
http_proxy="http://foo.bar:4444" ncytm
```

### Theming
The color palette can be modified in the configuration. For instance:

```toml
[theme]
background = "black"
primary = "light white"
secondary = "light black"
title = "red"
playing = "red"
playing_selected = "light red"
playing_bg = "black"
highlight = "light white"
highlight_bg = "#484848"
error = "light white"
error_bg = "red"
statusbar = "black"
statusbar_progress = "red"
statusbar_bg = "red"
cmdline = "light white"
cmdline_bg = "black"
search_match = "light red"
```

### Track Formatting
It's possible to customize how tracks are shown in Queue/Library views and the
statusbar, whereas `statusbar_format` will hold the statusbar formatting and
`[track_format]` the formatting for tracks in list views.
If you don't define `center` for example, the default value will be used.
Available options for tracks: `%artists`, `%artist`, `%title`, `%album`, `%saved`,
`%duration`.
`%artists` will show all contributing artists, while `%artist` only shows the first listed artist.

Default configuration:

```toml
statusbar_format = "%artists - %title"

[track_format]
left = "%artists - %title"
center = "%album"
right = "%saved %duration"
```

### Notification Formatting
`ncytm` also supports customizing the way notifications are displayed
(which appear when compiled with the `notify` feature and `notify = true`).
The title and body of the notification can be set, with `title` and `body`, or the default will be used.
The formatting options are the same as those for [track formatting](#track-formatting) (`%artists`, `%title`, etc)

Default configuration:

```toml
[notification_format]
title = "%title"
body = "%artists"
```

### Cover Drawing
When compiled with the `cover` feature, `ncytm` can draw the album art of the
current track in a dedicated view (`:focus cover` or <kbd>F8</kbd> by default)
using Überzug. The original project has been abandoned, therefore using a
compatible implementation such as [Überzug++](https://github.com/jstkdng/ueberzugpp)
is recommended. For more information on installation and terminal
compatibility, consult that repository.

To allow scaling up the album art beyond its native resolution, use the config
key `cover_max_scale`. This is especially useful for HiDPI displays:

```toml
cover_max_scale = 2
```

## Authentication
ncytm uses cookie-based authentication. You'll need to copy your YouTube Music cookies from your browser to authenticate.

*Detailed authentication instructions coming soon.*

The credentials are stored in `credentials.json` in the user's cache directory. Run
`ncytm info` to show the location of this directory.

The `logout` command can be used to remove cached credentials. See
[Vim-Like Commands](#vim-like-commands).
