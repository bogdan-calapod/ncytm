use std::collections::HashSet;
use std::io::Write;
use std::path::PathBuf;

use std::sync::{Arc, RwLock};

use cursive::theme::{ColorStyle, ColorType, PaletteColor};
use cursive::{Cursive, Printer, Vec2, View};
use ioctl_rs::{TIOCGWINSZ, ioctl};
use log::{debug, error};

use crate::command::{Command, GotoMode};
use crate::commands::CommandResult;
use crate::config::Config;
use crate::library::Library;
use crate::queue::Queue;
use crate::traits::{IntoBoxedViewExt, ListItem, ViewExt};
use crate::ui::album::AlbumView;
use crate::ui::artist::ArtistView;

pub struct CoverView {
    queue: Arc<Queue>,
    library: Arc<Library>,
    loading: Arc<RwLock<HashSet<String>>>,
    desired_cover: RwLock<Option<CoverRequest>>,
    rendered_cover: RwLock<Option<CoverRequest>>,
    cover_max_scale: Option<f32>,
    font_size: Vec2,
    /// Whether the terminal viewport hosting this image is currently on screen
    /// (e.g. our tmux pane is active). When false the image is cleared.
    visible: RwLock<bool>,
    /// Tracks the previous visibility to detect hidden->visible transitions.
    was_visible: RwLock<bool>,
    /// When set, keep re-asserting the image until this instant. Used after
    /// becoming visible again so the redraw survives the terminal's own screen
    /// replay on tmux (re)attach.
    redraw_until: RwLock<Option<std::time::Instant>>,
    /// Timestamp of the last re-assert emit, to throttle re-emits during the
    /// redraw window (the event loop does not sleep).
    last_reassert: RwLock<Option<std::time::Instant>>,
    /// When set, keep attempting to clear the image until this instant. Used
    /// after becoming hidden: the clear escape may be dropped while our tmux
    /// pane/session is off screen, so we retry in case output briefly flows.
    clear_until: RwLock<Option<std::time::Instant>>,
    /// Timestamp of the last clear attempt, to throttle clear retries.
    last_clear: RwLock<Option<std::time::Instant>>,
}

#[derive(Clone, PartialEq, Eq)]
struct CoverRequest {
    url: String,
    path: PathBuf,
    offset: Vec2,
    size: Vec2,
}

impl CoverView {
    pub fn new(queue: Arc<Queue>, library: Arc<Library>, config: &Config) -> Self {
        // Determine size of window both in pixels and chars
        let (rows, cols, xpixels, ypixels) = unsafe {
            let mut query: (u16, u16, u16, u16) = (0, 0, 0, 0);
            ioctl(1, TIOCGWINSZ, &mut query);
            query
        };

        debug!("Determined window dimensions: {xpixels}x{ypixels}, {cols}x{rows}");

        // Determine font size. Some terminals report physical pixels here, but
        // the aspect ratio is still useful when mapping images to terminal cells.
        let font_size = if cols == 0 || rows == 0 || xpixels == 0 || ypixels == 0 {
            Vec2::new(8, 16)
        } else {
            Vec2::new(
                std::cmp::max(1, xpixels / cols) as usize,
                std::cmp::max(1, ypixels / rows) as usize,
            )
        };

        debug!("Determined font size: {}x{}", font_size.x, font_size.y);

        Self {
            queue,
            library,
            loading: Arc::new(RwLock::new(HashSet::new())),
            desired_cover: RwLock::new(None),
            rendered_cover: RwLock::new(None),
            cover_max_scale: config.values().cover_max_scale,
            font_size,
            visible: RwLock::new(true),
            was_visible: RwLock::new(true),
            redraw_until: RwLock::new(None),
            last_reassert: RwLock::new(None),
            clear_until: RwLock::new(None),
            last_clear: RwLock::new(None),
        }
    }

    /// Set whether the hosting terminal viewport is on screen. When hidden, the
    /// next `render_to_terminal` call clears the image; the desired cover is
    /// preserved so it redraws once visible again.
    pub fn set_visible(&self, visible: bool) {
        *self.visible.write().unwrap() = visible;
    }

    fn draw_cover(&self, url: String, mut draw_offset: Vec2, draw_size: Vec2) {
        if draw_size.x <= 1 || draw_size.y <= 1 {
            return;
        }

        let path = match self.cache_path(url.clone()) {
            Some(p) => p,
            None => return,
        };

        let image_size = image::image_dimensions(&path).unwrap_or((640, 640));
        let mut size = self.cover_size(draw_size, image_size);

        // Make sure there is equal space in chars on either side
        if size.x > 1 && size.x % 2 != draw_size.x % 2 {
            size.x -= 1;
        }

        // Make sure x is the bottleneck so full width is used
        size.y = std::cmp::min(draw_size.y, size.y + 1);

        // Round up since the bottom might have empty space within
        // the designated box
        draw_offset.x += (draw_size.x - size.x) / 2;
        draw_offset.y += (draw_size.y - size.y) - (draw_size.y - size.y) / 2;

        let mut desired_cover = self.desired_cover.write().unwrap();
        *desired_cover = Some(CoverRequest {
            url,
            path,
            offset: draw_offset,
            size,
        });
    }

    fn clear_cover(&self) {
        let mut desired_cover = self.desired_cover.write().unwrap();
        *desired_cover = None;
    }

    fn cover_size(&self, draw_size: Vec2, image_size: (u32, u32)) -> Vec2 {
        let (image_width, image_height) = image_size;
        if image_width == 0 || image_height == 0 {
            return draw_size;
        }

        let mut available_size = draw_size;
        if let Some(scale) = self.cover_max_scale {
            let max_size = Vec2::new(
                ((image_width as f32 * scale) / self.font_size.x as f32) as usize,
                ((image_height as f32 * scale) / self.font_size.y as f32) as usize,
            );
            available_size.x = std::cmp::min(available_size.x, std::cmp::max(1, max_size.x));
            available_size.y = std::cmp::min(available_size.y, std::cmp::max(1, max_size.y));
        }

        fit_image_to_cells(available_size, self.font_size, image_width, image_height)
    }

    fn cache_path(&self, url: String) -> Option<PathBuf> {
        cache_path(&self.loading, url)
    }

    pub fn render_to_terminal(&self) {
        // When the hosting viewport is off screen, the effective target is
        // "nothing drawn" while the desired cover is kept for when we return.
        let visible = *self.visible.read().unwrap();
        let desired_cover = if visible {
            self.desired_cover.read().unwrap().clone()
        } else {
            None
        };

        // Detect visibility transitions. Terminal graphics are drawn to the
        // physical terminal and aren't tracked by tmux, so switching our
        // pane/session off screen leaves the image frozen, and any escape we
        // emit while off screen may be dropped. We therefore retry over short
        // windows: clearing after becoming hidden, and redrawing after becoming
        // visible.
        let (just_hidden, just_visible) = {
            let mut was_visible = self.was_visible.write().unwrap();
            let hidden = !visible && *was_visible;
            let shown = visible && !*was_visible;
            *was_visible = visible;
            (hidden, shown)
        };

        let now = std::time::Instant::now();
        if just_visible {
            reset_all_graphics();
            *self.rendered_cover.write().unwrap() = None;
            *self.redraw_until.write().unwrap() =
                Some(now + std::time::Duration::from_millis(1000));
            *self.clear_until.write().unwrap() = None;
        }
        if just_hidden {
            *self.clear_until.write().unwrap() = Some(now + std::time::Duration::from_millis(1000));
            *self.redraw_until.write().unwrap() = None;
        }

        // While hidden, keep retrying the clear (throttled) in case terminal
        // output briefly flows during the switch.
        if !visible && should_reassert(&self.clear_until, &self.last_clear) {
            reset_all_graphics();
        }

        // While inside the post-visible window, periodically re-emit the image
        // (without a delete/clear, so no flicker) even if nothing changed.
        let reassert = visible && should_reassert(&self.redraw_until, &self.last_reassert);

        let mut rendered_cover = self.rendered_cover.write().unwrap();

        if *rendered_cover == desired_cover {
            if reassert
                && let Some(cover) = desired_cover.as_ref()
                && let Err(e) = render_cover_to_terminal(cover)
            {
                error!("Failed to draw cover: {e}");
            }
            return;
        }

        if let Some(rendered) = rendered_cover.as_ref() {
            clear_terminal_area(rendered.offset, rendered.size);
        }

        if let Some(cover) = desired_cover.as_ref()
            && let Err(e) = render_cover_to_terminal(cover)
        {
            error!("Failed to draw cover: {e}");
            return;
        }

        *rendered_cover = desired_cover;
    }
}

/// Resolve the on-disk cache path for a cover `url`, downloading it in a
/// background thread if it is not present yet. Returns `None` while the image
/// is still downloading.
fn cache_path(loading: &Arc<RwLock<HashSet<String>>>, url: String) -> Option<PathBuf> {
    let path = crate::utils::cache_path_for_url(url.clone());

    let mut guard = loading.write().unwrap();
    if guard.contains(&url) {
        return None;
    }

    if path.exists() {
        return Some(path);
    }

    guard.insert(url.clone());

    let loading_thread = loading.clone();
    std::thread::spawn(move || {
        if let Err(e) = crate::utils::download(url.clone(), path.clone()) {
            error!("Failed to download cover: {e}");
        }
        let mut guard = loading_thread.write().unwrap();
        guard.remove(&url.clone());
    });

    None
}

fn render_cover_to_terminal(cover: &CoverRequest) -> Result<(), viuer::ViuError> {
    // In Kitty terminals we render the Kitty graphics protocol ourselves.
    // viuer's built-in Kitty support probes the terminal with an escape query,
    // which is swallowed by multiplexers like tmux, so viuer would otherwise
    // fall back to low-resolution Sixel/blocks. Rendering directly lets us wrap
    // the escapes in tmux passthrough and send the full-resolution image.
    if is_kitty_terminal() && can_use_kitty_graphics() {
        match render_cover_kitty(cover) {
            Ok(()) => return Ok(()),
            Err(e) => {
                debug!("Native Kitty render failed, falling back to viuer: {e}");
            }
        }
    }

    let config = viuer::Config {
        x: to_u16(cover.offset.x)?,
        y: to_i16(cover.offset.y)?,
        width: Some(cover.size.x as u32),
        height: Some(cover.size.y as u32),
        absolute_offset: true,
        restore_cursor: true,
        use_kitty: can_use_kitty_graphics(),
        // viuer's `choose_printer` prefers Sixel over Kitty when both are
        // enabled, so disable Sixel when the Kitty graphics protocol is
        // available to avoid emitting raw Sixel data that Kitty renders as a
        // garbled blob.
        use_sixel: !is_iterm_terminal() && !is_kitty_terminal(),
        ..Default::default()
    };

    let image = image::ImageReader::open(&cover.path)?
        .with_guessed_format()?
        .decode()?;

    viuer::print(&image, &config).map(|_| ())
}

/// Render a cover using the Kitty graphics protocol directly.
///
/// The full-resolution image data is sent and Kitty scales it into the
/// requested cell box (`c`/`r`), so the result stays crisp regardless of the
/// on-screen size. Escapes are wrapped for tmux passthrough when needed.
fn render_cover_kitty(cover: &CoverRequest) -> Result<(), Box<dyn std::error::Error>> {
    use base64::Engine;

    // Decode the image (JPEG/PNG/...) and re-encode as PNG. Kitty only accepts
    // PNG (f=100) or raw RGB/RGBA (f=24/f=32) — not JPEG — so we must decode
    // rather than forward the file bytes. PNG keeps the payload small, which
    // matters for throughput through tmux passthrough.
    let image = image::ImageReader::open(&cover.path)?
        .with_guessed_format()?
        .decode()?;
    let mut png_bytes: Vec<u8> = Vec::new();
    image.write_to(
        &mut std::io::Cursor::new(&mut png_bytes),
        image::ImageFormat::Png,
    )?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&png_bytes);

    let mut stdout = std::io::stdout();

    // Save cursor, position at the absolute target cell (1-based). We restore
    // the cursor afterwards so Kitty's post-placement cursor movement does not
    // disturb the TUI. The cursor control codes are plain (not graphics), so
    // they are sent without tmux passthrough.
    write!(
        stdout,
        "\x1b[s\x1b[{};{}H",
        cover.offset.y + 1,
        cover.offset.x + 1
    )?;

    // Kitty graphics: transmit-and-display (a=T) a PNG (f=100), scaled into
    // c columns by r rows. The base64 payload is chunked in <=4096-byte pieces.
    let cols = cover.size.x;
    let rows = cover.size.y;
    let chunk_size = 4096;
    let chunks: Vec<&str> = encoded
        .as_bytes()
        .chunks(chunk_size)
        .map(|c| std::str::from_utf8(c).unwrap())
        .collect();

    if chunks.is_empty() {
        write!(stdout, "\x1b[u")?;
        return Ok(());
    }

    for (i, chunk) in chunks.iter().enumerate() {
        let last = i == chunks.len() - 1;
        let more = if last { 0 } else { 1 };
        let control = if i == 0 {
            format!("a=T,f=100,c={cols},r={rows},q=2,m={more}")
        } else {
            format!("m={more}")
        };
        kitty_write(&mut stdout, &format!("\x1b_G{control};{chunk}\x1b\\"))?;
    }

    // Restore the cursor to where it was before we moved it.
    write!(stdout, "\x1b[u")?;

    stdout.flush()?;
    Ok(())
}

/// Write a terminal control string, wrapping it in the tmux passthrough
/// sequence when running inside tmux so the outer terminal receives it.
fn kitty_write(stdout: &mut impl Write, payload: &str) -> std::io::Result<()> {
    if is_tmux() {
        // tmux passthrough: ESC P tmux; <payload with ESC doubled> ESC \
        let escaped = payload.replace('\x1b', "\x1b\x1b");
        write!(stdout, "\x1bPtmux;{escaped}\x1b\\")
    } else {
        stdout.write_all(payload.as_bytes())
    }
}

fn is_tmux() -> bool {
    std::env::var_os("TMUX").is_some()
        || std::env::var("TERM").is_ok_and(|term| term.starts_with("tmux"))
        || std::env::var("TERM_PROGRAM").is_ok_and(|term| term == "tmux")
}

/// Whether ncytm's tmux pane is the one currently visible/active. Terminal
/// graphics are drawn to the physical terminal and are not tracked by tmux, so
/// we must hide them when our pane is not on screen. This is the case when a
/// different pane is focused, a different window is selected, or the session is
/// not currently attached to any client (e.g. a client switched to another
/// session). Returns true when not running under tmux.
pub fn tmux_pane_visible() -> bool {
    if !is_tmux() {
        return true;
    }

    let our_pane = match std::env::var("TMUX_PANE") {
        Ok(p) if !p.is_empty() => p,
        _ => return true,
    };

    // Query tmux for: our pane is active, its window is active, and the session
    // has at least one attached client currently viewing it.
    let output = std::process::Command::new("tmux")
        .args([
            "display-message",
            "-p",
            "-t",
            &our_pane,
            "#{&&:#{window_active},#{&&:#{pane_active},#{session_attached}}}",
        ])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let s = String::from_utf8_lossy(&out.stdout);
            let value = s.trim();
            // The expression yields "0" when any condition is false, otherwise
            // "1" when the pane/window are active and the session is attached.
            let visible = value != "0" && !value.is_empty();
            debug!("tmux_pane_visible: pane={our_pane} query={value:?} visible={visible}");
            visible
        }
        // If the query fails, assume visible so we don't permanently hide.
        other => {
            debug!("tmux_pane_visible: query failed ({other:?}), assuming visible");
            true
        }
    }
}

fn is_iterm_terminal() -> bool {
    std::env::var("TERM_PROGRAM").is_ok_and(|term| term.contains("iTerm"))
        || std::env::var("LC_TERMINAL").is_ok_and(|term| term.contains("iTerm"))
}

fn is_apple_terminal() -> bool {
    std::env::var("TERM_PROGRAM").is_ok_and(|term| term == "Apple_Terminal")
}

fn is_kitty_terminal() -> bool {
    std::env::var_os("KITTY_WINDOW_ID").is_some()
        || std::env::var("TERM").is_ok_and(|term| term.contains("kitty"))
        || std::env::var("TERM_PROGRAM").is_ok_and(|term| term.contains("kitty"))
}

fn can_use_kitty_graphics() -> bool {
    !is_apple_terminal()
}

/// Whether a graphics protocol that supports absolute positioning is available.
///
/// The floating status thumbnail overlays other views, so it relies on a real
/// image protocol (Kitty or iTerm2). Block/half-cell fallback rendering writes
/// text cells that Cursive would overwrite and that can't float, so the
/// thumbnail is disabled when only that fallback is available.
pub fn status_thumbnail_supported() -> bool {
    (is_kitty_terminal() && can_use_kitty_graphics()) || is_iterm_terminal()
}

fn fit_image_to_cells(
    available_size: Vec2,
    font_size: Vec2,
    image_width: u32,
    image_height: u32,
) -> Vec2 {
    if available_size.x == 0 || available_size.y == 0 || font_size.x == 0 || font_size.y == 0 {
        return Vec2::new(0, 0);
    }

    let image_aspect = image_width as f32 / image_height as f32;
    let cell_aspect = font_size.x as f32 / font_size.y as f32;
    let width_for_full_height =
        (available_size.y as f32 * image_aspect / cell_aspect).floor() as usize;

    if width_for_full_height <= available_size.x {
        Vec2::new(std::cmp::max(1, width_for_full_height), available_size.y)
    } else {
        let height_for_full_width =
            (available_size.x as f32 * cell_aspect / image_aspect).floor() as usize;
        Vec2::new(available_size.x, std::cmp::max(1, height_for_full_width))
    }
}

/// Minimum interval between re-assert emits during the post-visible window.
const REASSERT_INTERVAL: std::time::Duration = std::time::Duration::from_millis(150);

/// Decide whether to re-emit the image during the post-visible redraw window.
///
/// Returns true at most once per [`REASSERT_INTERVAL`] while the current time
/// is before `redraw_until`; clears the window once it elapses. The event loop
/// does not sleep, so this throttling prevents flooding the terminal with
/// hundreds of full-image redraws per second.
fn should_reassert(
    redraw_until: &RwLock<Option<std::time::Instant>>,
    last_reassert: &RwLock<Option<std::time::Instant>>,
) -> bool {
    let now = std::time::Instant::now();

    {
        let mut until = redraw_until.write().unwrap();
        match *until {
            Some(deadline) if now < deadline => {}
            Some(_) => {
                *until = None;
                return false;
            }
            None => return false,
        }
    }

    let mut last = last_reassert.write().unwrap();
    if last
        .map(|t| now.duration_since(t) >= REASSERT_INTERVAL)
        .unwrap_or(true)
    {
        *last = Some(now);
        true
    } else {
        false
    }
}

/// Delete all Kitty graphics from the terminal. Used when returning to
/// visibility after being hidden, to purge any image that may have been frozen
/// on screen while our output could not reach the terminal (e.g. a detached
/// tmux session).
fn reset_all_graphics() {
    if !can_use_kitty_graphics() {
        return;
    }
    let mut stdout = std::io::stdout();
    let _ = kitty_write(&mut stdout, "\x1b_Ga=d,d=A\x1b\\");
    let _ = stdout.flush();
}

fn clear_terminal_area(offset: Vec2, size: Vec2) {
    let mut stdout = std::io::stdout();

    // Remove stateful Kitty graphics where that protocol is available, then
    // overwrite the cells used by other protocols/fallbacks. The delete escape
    // is wrapped for tmux passthrough so it reaches Kitty through tmux.
    if can_use_kitty_graphics() {
        let _ = kitty_write(&mut stdout, "\x1b_Ga=d,d=A\x1b\\");
    }
    for y in offset.y..offset.y + size.y {
        let _ = write!(
            stdout,
            "\x1b[{};{}H{}",
            y + 1,
            offset.x + 1,
            " ".repeat(size.x)
        );
    }
    let _ = stdout.flush();
}

fn to_u16(value: usize) -> Result<u16, viuer::ViuError> {
    u16::try_from(value).map_err(|_| {
        viuer::ViuError::InvalidConfiguration("cover coordinate is too large".to_string())
    })
}

fn to_i16(value: usize) -> Result<i16, viuer::ViuError> {
    i16::try_from(value).map_err(|_| {
        viuer::ViuError::InvalidConfiguration("cover coordinate is too large".to_string())
    })
}

impl View for CoverView {
    fn draw(&self, printer: &Printer<'_, '_>) {
        // Completely blank out screen
        let style = ColorStyle::new(
            ColorType::Palette(PaletteColor::Background),
            ColorType::Palette(PaletteColor::Background),
        );
        printer.with_color(style, |printer| {
            for i in 0..printer.size.y {
                printer.print_hline((0, i), printer.size.x, " ");
            }
        });

        let cover_url = self.queue.get_current().and_then(|t| t.cover_url());

        if let Some(url) = cover_url {
            self.draw_cover(url, printer.offset, printer.size);
        } else {
            self.clear_cover();
        }
    }

    fn required_size(&mut self, constraint: Vec2) -> Vec2 {
        Vec2::new(constraint.x, 2)
    }
}

impl ViewExt for CoverView {
    fn title(&self) -> String {
        "Cover".to_string()
    }

    fn on_leave(&self) {
        self.clear_cover();
        self.render_to_terminal();
    }

    fn on_command(&mut self, _s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        match cmd {
            Command::Save => {
                if let Some(mut track) = self.queue.get_current() {
                    track.save(&self.library);
                }
            }
            Command::Delete => {
                if let Some(mut track) = self.queue.get_current() {
                    track.unsave(&self.library);
                }
            }
            #[cfg(feature = "share_clipboard")]
            Command::Share(_mode) => {
                let url = self
                    .queue
                    .get_current()
                    .and_then(|t| t.as_listitem().share_url());

                if let Some(url) = url {
                    crate::sharing::write_share(url).ok();
                }

                return Ok(CommandResult::Consumed(None));
            }
            Command::Goto(mode) => {
                if let Some(track) = self.queue.get_current() {
                    let queue = self.queue.clone();
                    let library = self.library.clone();

                    match mode {
                        GotoMode::Album => {
                            if let Some(album) = track.album(&queue) {
                                let view =
                                    AlbumView::new(queue, library, &album).into_boxed_view_ext();
                                return Ok(CommandResult::View(view));
                            }
                        }
                        GotoMode::Artist => {
                            if let Some(artists) = track.artists() {
                                return match artists.len() {
                                    0 => Ok(CommandResult::Consumed(None)),
                                    // Always choose the first artist even with more because
                                    // the cover image really doesn't play nice with the menu
                                    _ => {
                                        let view = ArtistView::new(queue, library, &artists[0])
                                            .into_boxed_view_ext();
                                        Ok(CommandResult::View(view))
                                    }
                                };
                            }
                        }
                    }
                }
            }
            _ => {}
        };

        Ok(CommandResult::Ignored)
    }
}

/// Default height (in terminal rows) of the floating status thumbnail.
const DEFAULT_STATUS_COVER_HEIGHT: usize = 6;

/// A small album-art thumbnail that floats in the bottom-right corner, just
/// above the statusbar. Unlike [`CoverView`] it is not a Cursive screen; it is
/// rendered directly to the terminal after each frame flush, using the same
/// image protocols as the full cover.
pub struct StatusCover {
    queue: Arc<Queue>,
    loading: Arc<RwLock<HashSet<String>>>,
    desired: RwLock<Option<CoverRequest>>,
    rendered: RwLock<Option<CoverRequest>>,
    font_size: Vec2,
    height: usize,
    /// Whether the thumbnail was shown (not suppressed) on the previous update,
    /// used to detect a suppressed->visible transition and force a redraw.
    was_visible: RwLock<bool>,
    /// Set when we transition back to visible so the next render fully resets
    /// terminal graphics before redrawing (see [`CoverView::render_to_terminal`]).
    force_reset: RwLock<bool>,
    /// Keep re-asserting the image until this instant after becoming visible,
    /// so the redraw survives the terminal's screen replay on (re)attach.
    redraw_until: RwLock<Option<std::time::Instant>>,
    /// Timestamp of the last re-assert emit, to throttle re-emits.
    last_reassert: RwLock<Option<std::time::Instant>>,
}

impl StatusCover {
    pub fn new(queue: Arc<Queue>, config: &Config) -> Self {
        let font_size = detect_font_size();
        let height = config
            .values()
            .status_cover_size
            .filter(|h| *h > 0)
            .unwrap_or(DEFAULT_STATUS_COVER_HEIGHT);

        Self {
            queue,
            loading: Arc::new(RwLock::new(HashSet::new())),
            desired: RwLock::new(None),
            rendered: RwLock::new(None),
            font_size,
            height,
            was_visible: RwLock::new(true),
            force_reset: RwLock::new(false),
            redraw_until: RwLock::new(None),
            last_reassert: RwLock::new(None),
        }
    }

    /// Recompute the desired thumbnail based on the current track. When
    /// `suppress` is true (e.g. the full cover screen is open, or our tmux pane
    /// is not on screen) the thumbnail is hidden.
    pub fn update(&self, suppress: bool) {
        let visible = !suppress;

        // Detect a transition back to visible. While hidden any clear escape we
        // emit is dropped (e.g. detached tmux session), so a stale image may be
        // frozen on the terminal. Force a full reset and keep re-asserting the
        // image for a short window so it survives the terminal's replay.
        {
            let mut was_visible = self.was_visible.write().unwrap();
            if visible && !*was_visible {
                *self.force_reset.write().unwrap() = true;
                *self.redraw_until.write().unwrap() =
                    Some(std::time::Instant::now() + std::time::Duration::from_millis(1000));
            }
            *was_visible = visible;
        }

        let url = if suppress {
            None
        } else {
            self.queue.get_current().and_then(|t| t.cover_url())
        };

        let request = url.and_then(|url| self.build_request(url));

        let mut desired = self.desired.write().unwrap();
        *desired = request;
    }

    fn build_request(&self, url: String) -> Option<CoverRequest> {
        let (term_cols, term_rows) = terminal_cell_size();
        if term_cols == 0 || term_rows == 0 {
            return None;
        }

        let path = cache_path(&self.loading, url.clone())?;

        let image_size = image::image_dimensions(&path).unwrap_or((640, 640));
        // Fit a square-ish box of the requested height, preserving aspect ratio.
        let max_box = Vec2::new(term_cols, self.height);
        let size = fit_image_to_cells(max_box, self.font_size, image_size.0, image_size.1);
        if size.x == 0 || size.y == 0 {
            return None;
        }

        // Anchor in the bottom-right, on the rows directly above the statusbar
        // (2 rows) plus the persistent command/result line (1 row), so the
        // thumbnail never overlaps the time indicator or volume text.
        const STATUSBAR_HEIGHT: usize = 2;
        const CMDLINE_HEIGHT: usize = 1;
        let bottom_reserved = STATUSBAR_HEIGHT + CMDLINE_HEIGHT;

        if term_rows <= bottom_reserved + size.y {
            return None;
        }

        let offset = Vec2::new(
            term_cols.saturating_sub(size.x),
            term_rows - bottom_reserved - size.y,
        );

        Some(CoverRequest {
            url,
            path,
            offset,
            size,
        })
    }

    /// Draw or clear the thumbnail to the terminal, diffing against the last
    /// rendered request to avoid redundant redraws.
    pub fn render_to_terminal(&self) {
        let desired = self.desired.read().unwrap().clone();

        let force_reset = {
            let mut fr = self.force_reset.write().unwrap();
            let v = *fr;
            *fr = false;
            v
        };

        if force_reset {
            reset_all_graphics();
            *self.rendered.write().unwrap() = None;
        }

        // While inside the post-visible window, periodically re-emit the image
        // (without a delete/clear, so no flicker) even if nothing changed.
        let reassert = should_reassert(&self.redraw_until, &self.last_reassert);

        let mut rendered = self.rendered.write().unwrap();

        if *rendered == desired {
            if reassert
                && let Some(cover) = desired.as_ref()
                && let Err(e) = render_cover_to_terminal(cover)
            {
                error!("Failed to draw status cover: {e}");
            }
            return;
        }

        if let Some(prev) = rendered.as_ref() {
            clear_terminal_area(prev.offset, prev.size);
        }

        if let Some(cover) = desired.as_ref()
            && let Err(e) = render_cover_to_terminal(cover)
        {
            error!("Failed to draw status cover: {e}");
            return;
        }

        *rendered = desired;
    }

    /// Clear any drawn thumbnail (used on shutdown).
    pub fn clear(&self) {
        {
            let mut desired = self.desired.write().unwrap();
            *desired = None;
        }
        self.render_to_terminal();
    }
}

/// Determine the pixel size of a single terminal cell, used to map image
/// pixels to terminal cells while preserving aspect ratio.
fn detect_font_size() -> Vec2 {
    let (rows, cols, xpixels, ypixels) = unsafe {
        let mut query: (u16, u16, u16, u16) = (0, 0, 0, 0);
        ioctl(1, TIOCGWINSZ, &mut query);
        query
    };

    if cols == 0 || rows == 0 || xpixels == 0 || ypixels == 0 {
        Vec2::new(8, 16)
    } else {
        Vec2::new(
            std::cmp::max(1, xpixels / cols) as usize,
            std::cmp::max(1, ypixels / rows) as usize,
        )
    }
}

/// Current terminal size in cells (cols, rows).
fn terminal_cell_size() -> (usize, usize) {
    let (rows, cols, _xpixels, _ypixels) = unsafe {
        let mut query: (u16, u16, u16, u16) = (0, 0, 0, 0);
        ioctl(1, TIOCGWINSZ, &mut query);
        query
    };
    (cols as usize, rows as usize)
}
