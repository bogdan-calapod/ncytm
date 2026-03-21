#![allow(clippy::use_self)]

use log::info;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::UnboundedReceiverStream;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, Value};
use zbus::{connection, interface};

use crate::application::ASYNC_RUNTIME;
use crate::library::Library;
use crate::model::album::Album;
use crate::model::episode::Episode;
use crate::model::playable::Playable;
use crate::model::playlist::Playlist;
use crate::model::show::Show;
use crate::model::track::Track;
use crate::queue::RepeatSetting;
use crate::spotify::UriType;
use crate::traits::ListItem;
use crate::youtube_url::YouTubeUrl;
use crate::{
    events::EventManager,
    queue::Queue,
    spotify::{PlayerEvent, Spotify, VOLUME_PERCENT},
};

struct MprisRoot {}

#[interface(name = "org.mpris.MediaPlayer2")]
impl MprisRoot {
    #[zbus(property)]
    fn can_quit(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn has_tracklist(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn identity(&self) -> &str {
        "ncytm"
    }

    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        vec!["youtube".to_string(), "https".to_string()]
    }

    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        Vec::new()
    }

    fn raise(&self) {}

    fn quit(&self) {}
}

struct MprisPlayer {
    event: EventManager,
    queue: Arc<Queue>,
    library: Arc<Library>,
    spotify: Spotify,
}

#[interface(name = "org.mpris.MediaPlayer2.Player")]
impl MprisPlayer {
    #[zbus(property)]
    fn playback_status(&self) -> &str {
        match self.spotify.get_current_status() {
            // Loading is reported as Playing since we're about to play
            PlayerEvent::Loading | PlayerEvent::Playing(_) => "Playing",
            PlayerEvent::Paused(_) => "Paused",
            PlayerEvent::Stopped => "Stopped",
        }
    }

    #[zbus(property)]
    fn loop_status(&self) -> &str {
        match self.queue.get_repeat() {
            RepeatSetting::None => "None",
            RepeatSetting::RepeatTrack => "Track",
            RepeatSetting::RepeatPlaylist => "Playlist",
        }
    }

    #[zbus(property)]
    fn set_loop_status(&self, loop_status: &str) {
        let setting = match loop_status {
            "Track" => RepeatSetting::RepeatTrack,
            "Playlist" => RepeatSetting::RepeatPlaylist,
            _ => RepeatSetting::None,
        };
        self.queue.set_repeat(setting);
        self.event.trigger();
    }

    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, Value<'_>> {
        let mut hm = HashMap::new();

        let playable = self.queue.get_current();

        // Fetch full track details in case this playable is based on a SimplifiedTrack
        // This is necessary because SimplifiedTrack objects don't contain a cover_url
        let playable_full = playable.and_then(|p| match p {
            Playable::Track(track) => {
                if track.cover_url.is_some() {
                    // We already have `cover_url`, no need to fetch the full track
                    Some(Playable::Track(track))
                } else {
                    self.spotify
                        .api
                        .track(&track.id.unwrap_or_default())
                        .map(Playable::Track)
                }
            }
            Playable::Episode(episode) => Some(Playable::Episode(episode)),
        });
        let playable = playable_full.as_ref();

        hm.insert(
            "mpris:trackid".to_string(),
            Value::ObjectPath(ObjectPath::from_string_unchecked(format!(
                "/org/ncytm/{}",
                playable
                    .filter(|t| t.id().is_some())
                    .map(|t| t.uri().replace(':', "/"))
                    .unwrap_or_else(|| String::from("0"))
            ))),
        );
        hm.insert(
            "mpris:length".to_string(),
            Value::I64(playable.map(|t| t.duration() as i64 * 1_000).unwrap_or(0)),
        );
        hm.insert(
            "mpris:artUrl".to_string(),
            Value::Str(
                playable
                    .map(|t| t.cover_url().unwrap_or_default())
                    .unwrap_or_default()
                    .into(),
            ),
        );

        hm.insert(
            "xesam:album".to_string(),
            Value::Str(
                playable
                    .and_then(|p| p.track())
                    .map(|t| t.album.unwrap_or_default())
                    .unwrap_or_default()
                    .into(),
            ),
        );
        hm.insert(
            "xesam:albumArtist".to_string(),
            Value::Array(
                playable
                    .and_then(|p| p.track())
                    .map(|t| t.artists.clone())
                    .unwrap_or_default()
                    .into(),
            ),
        );
        hm.insert(
            "xesam:artist".to_string(),
            Value::Array(
                playable
                    .and_then(|p| p.track())
                    .map(|t| t.artists)
                    .unwrap_or_default()
                    .into(),
            ),
        );
        hm.insert(
            "xesam:discNumber".to_string(),
            Value::I32(1), // YouTube Music doesn't have disc numbers
        );
        hm.insert(
            "xesam:title".to_string(),
            Value::Str(
                playable
                    .map(|t| match t {
                        Playable::Track(t) => t.title.clone(),
                        Playable::Episode(ep) => ep.name.clone(),
                    })
                    .unwrap_or_default()
                    .into(),
            ),
        );
        hm.insert(
            "xesam:trackNumber".to_string(),
            Value::I32(0), // YouTube Music doesn't expose track numbers in search/library
        );
        hm.insert(
            "xesam:url".to_string(),
            Value::Str(
                playable
                    .map(|t| t.share_url().unwrap_or_default())
                    .unwrap_or_default()
                    .into(),
            ),
        );
        hm.insert(
            "xesam:userRating".to_string(),
            Value::F64(
                playable
                    .and_then(|p| p.track())
                    .map(|t| match self.library.is_saved_track(&Playable::Track(t)) {
                        true => 1.0,
                        false => 0.0,
                    })
                    .unwrap_or(0.0),
            ),
        );

        hm
    }

    #[zbus(property)]
    fn shuffle(&self) -> bool {
        self.queue.get_shuffle()
    }

    #[zbus(property)]
    fn set_shuffle(&self, shuffle: bool) {
        self.queue.set_shuffle(shuffle);
        self.event.trigger();
    }

    #[zbus(property)]
    fn volume(&self) -> f64 {
        self.spotify.volume() as f64 / 65535_f64
    }

    #[zbus(property)]
    fn set_volume(&self, volume: f64) {
        log::info!("set volume: {volume}");
        let clamped = volume.clamp(0.0, 1.0);
        let vol = (VOLUME_PERCENT as f64) * clamped * 100.0;
        self.spotify.set_volume(vol as u16, false);
        self.event.trigger();
    }

    #[zbus(property)]
    fn position(&self) -> i64 {
        self.spotify.get_current_progress().as_micros() as i64
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        self.queue.next_index().is_some()
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        self.queue.get_current().is_some()
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        self.queue.get_current().is_some()
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        self.queue.get_current().is_some()
    }

    #[zbus(property)]
    fn can_seek(&self) -> bool {
        self.queue.get_current().is_some()
    }

    #[zbus(property)]
    fn can_control(&self) -> bool {
        self.queue.get_current().is_some()
    }

    #[zbus(signal)]
    async fn seeked(context: &SignalEmitter<'_>, position: &i64) -> zbus::Result<()>;

    fn next(&self) {
        self.queue.next(true)
    }

    fn previous(&self) {
        if self.spotify.get_current_progress() < Duration::from_secs(5) {
            self.queue.previous();
        } else {
            self.spotify.seek(0);
        }
    }

    fn pause(&self) {
        self.spotify.pause()
    }

    fn play_pause(&self) {
        self.queue.toggleplayback()
    }

    fn stop(&self) {
        self.queue.stop()
    }

    fn play(&self) {
        self.spotify.play()
    }

    fn seek(&self, offset: i64) {
        if let Some(current_track) = self.queue.get_current() {
            let progress = self.spotify.get_current_progress();
            let new_position = (progress.as_secs() * 1000) as i32
                + progress.subsec_millis() as i32
                + (offset / 1000) as i32;
            let new_position = new_position.max(0) as u32;
            let duration = current_track.duration();

            if new_position < duration {
                self.spotify.seek(new_position);
            } else {
                self.queue.next(true);
            }
        }
    }

    fn set_position(&self, _track: ObjectPath, position: i64) {
        if let Some(current_track) = self.queue.get_current() {
            let position = (position / 1000) as u32;
            let duration = current_track.duration();

            if position < duration {
                self.spotify.seek(position);
            }
        }
    }

    fn open_uri(&self, uri: &str) {
        let youtube_url = if uri.contains("music.youtube.com")
            || uri.contains("youtube.com")
            || uri.contains("youtu.be")
        {
            YouTubeUrl::from_url(uri)
        } else if let Ok(uri_type) = uri.parse() {
            let id = &uri[uri.rfind(':').unwrap_or(0) + 1..uri.len()];
            Some(YouTubeUrl::new(id, uri_type))
        } else {
            None
        };

        let id = youtube_url
            .as_ref()
            .map(|s| s.id.clone())
            .unwrap_or("".to_string());
        let uri_type = youtube_url.map(|s| s.uri_type);
        match uri_type {
            Some(UriType::Album) => {
                if let Ok(album) = self.spotify.api.album(&id) {
                    if let Some(t) = &album.tracks {
                        let should_shuffle = self.queue.get_shuffle();
                        self.queue.clear();
                        let index = self.queue.append_next(
                            &t.iter()
                                .map(|track| Playable::Track(track.clone()))
                                .collect(),
                        );
                        self.queue.play(index, should_shuffle, should_shuffle)
                    }
                }
            }
            Some(UriType::Track) => {
                if let Some(t) = self.spotify.api.track(&id) {
                    self.queue.clear();
                    self.queue.append(Playable::Track(t));
                    self.queue.play(0, false, false)
                }
            }
            Some(UriType::Playlist) => {
                if let Some(mut playlist) = self.spotify.api.playlist(&id) {
                    playlist.load_tracks(&self.spotify);
                    if let Some(tracks) = &playlist.tracks {
                        let should_shuffle = self.queue.get_shuffle();
                        self.queue.clear();
                        let index = self.queue.append_next(tracks);
                        self.queue.play(index, should_shuffle, should_shuffle)
                    }
                }
            }
            Some(UriType::Show) => {
                if let Some(mut show) = self.spotify.api.show(&id) {
                    let spotify = self.spotify.clone();
                    show.load_all_episodes(spotify);
                    if let Some(e) = &show.episodes {
                        let should_shuffle = self.queue.get_shuffle();
                        self.queue.clear();
                        let mut ep: Vec<Episode> = e.clone();
                        ep.reverse();
                        let index = self.queue.append_next(
                            &ep.iter()
                                .map(|episode| Playable::Episode(episode.clone()))
                                .collect(),
                        );
                        self.queue.play(index, should_shuffle, should_shuffle)
                    }
                }
            }
            Some(UriType::Episode) => {
                if let Some(e) = self.spotify.api.episode(&id) {
                    self.queue.clear();
                    self.queue.append(Playable::Episode(e));
                    self.queue.play(0, false, false)
                }
            }
            Some(UriType::Artist) => {
                let tracks = self.spotify.api.artist_top_tracks(&id);
                if !tracks.is_empty() {
                    let should_shuffle = self.queue.get_shuffle();
                    self.queue.clear();
                    let index = self.queue.append_next(
                        &tracks
                            .iter()
                            .map(|track| Playable::Track(track.clone()))
                            .collect(),
                    );
                    self.queue.play(index, should_shuffle, should_shuffle)
                }
            }
            None => {}
        }
    }
}

/// Commands to control the [MprisManager] worker thread.
#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
pub enum MprisCommand {
    /// Emit playback status
    EmitPlaybackStatus,
    /// Emit volume
    EmitVolumeStatus,
    /// Emit metadata
    EmitMetadataStatus,
    /// Emit seeked position
    EmitSeekedStatus(i64),
}

/// An MPRIS server that internally manager a thread which can be sent commands. This is internally
/// shared and cloning it will yield a reference to the same server.
#[derive(Clone)]
pub struct MprisManager {
    tx: mpsc::UnboundedSender<MprisCommand>,
}

impl MprisManager {
    pub fn new(
        event: EventManager,
        queue: Arc<Queue>,
        library: Arc<Library>,
        spotify: Spotify,
    ) -> Self {
        let root = MprisRoot {};
        let player = MprisPlayer {
            event,
            queue,
            library,
            spotify,
        };

        let (tx, rx) = mpsc::unbounded_channel::<MprisCommand>();

        ASYNC_RUNTIME.get().unwrap().spawn(async {
            let result = Self::serve(UnboundedReceiverStream::new(rx), root, player).await;
            if let Err(e) = result {
                log::error!("MPRIS error: {e}");
            }
        });

        Self { tx }
    }

    async fn serve(
        mut rx: UnboundedReceiverStream<MprisCommand>,
        root: MprisRoot,
        player: MprisPlayer,
    ) -> Result<(), Box<dyn Error + Sync + Send>> {
        let conn = connection::Builder::session()?
            .name(instance_bus_name())?
            .serve_at("/org/mpris/MediaPlayer2", root)?
            .serve_at("/org/mpris/MediaPlayer2", player)?
            .build()
            .await?;

        let object_server = conn.object_server();
        let player_iface_ref = object_server
            .interface::<_, MprisPlayer>("/org/mpris/MediaPlayer2")
            .await?;
        let player_iface = player_iface_ref.get().await;

        loop {
            let ctx = player_iface_ref.signal_emitter();
            match rx.next().await {
                Some(MprisCommand::EmitPlaybackStatus) => {
                    player_iface.playback_status_changed(ctx).await?;
                }
                Some(MprisCommand::EmitVolumeStatus) => {
                    info!("sending MPRIS volume update signal");
                    player_iface.volume_changed(ctx).await?;
                }
                Some(MprisCommand::EmitMetadataStatus) => {
                    player_iface.metadata_changed(ctx).await?;
                }
                Some(MprisCommand::EmitSeekedStatus(pos)) => {
                    info!("sending MPRIS seeked signal");
                    MprisPlayer::seeked(ctx, &pos).await?;
                }
                None => break,
            }
        }
        Err("MPRIS server command channel closed".into())
    }

    pub fn send(&self, command: MprisCommand) {
        if let Err(e) = self.tx.send(command) {
            log::warn!("Could not update MPRIS state: {e}");
        }
    }
}

/// Get the D-Bus bus name for this instance according to the MPRIS specification.
///
/// <https://specifications.freedesktop.org/mpris-spec/2.2/#Bus-Name-Policy>
pub fn instance_bus_name() -> String {
    format!(
        "org.mpris.MediaPlayer2.ncytm.instance{}",
        std::process::id()
    )
}
