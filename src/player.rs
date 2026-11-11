// SPDX-License-Identifier: GPL-3.0-or-later

//! A player that plays nothing and stands still: the song, its position, the volume, the
//! equalizer and the spectrum are what the parameters say, the same on every run.

use walkit::player::{Player, Status, Track};

use crate::params::{Params, StatusParam, VisModeParam};

/// The bands Winamp hands a visualizer.
const BANDS: usize = 75;

pub struct StillPlayer {
    status: Status,
    position_ms: i32,
    tracks: Vec<Track>,
    index: i32,
    kbps: i32,
    khz: i32,
    channels: i32,
    volume: i32,
    pan: i32,
    shuffle: bool,
    repeat: bool,
    crossfade: bool,
    eq_enabled: bool,
    eq_preamp: i32,
    eq_bands: Vec<i32>,
    spectrum: Vec<f32>,
    scope: bool,
}

/// A 0..100 slider (100 is +12 dB) as the engine keeps a band: 0..255 with 255 the top, which
/// it shows in Winamp's 64 steps.
fn eq_value(percent: i32) -> i32 {
    let step = (percent.clamp(0, 100) * 63 + 50) / 100;
    step * 4 + 2
}

/// The analyzer's levels for bars `bars` pixels tall. The engine draws a bar for every four
/// bands, at the mean of their levels in sixteenths, two rows of it under the picture, and
/// the peak mark one row above the bar.
fn spectrum(bars: &[f32], peaks: bool) -> Vec<f32> {
    let mut levels = vec![0.0; BANDS];
    for (bar, height) in bars.iter().enumerate() {
        // a bar that reaches into a row fills it
        let rows = height.ceil().clamp(0.0, 16.0) as i32;
        if rows == 0 {
            continue;
        }
        // with peak marks the mark is the bar's top row
        let level = (rows + if peaks { 1 } else { 2 }).min(15);
        for band in levels.iter_mut().skip(bar * 4).take(4) {
            *band = level as f32 / 15.0;
        }
    }
    levels
}

impl StillPlayer {
    pub fn new(params: &Params) -> Self {
        let p = &params.player;
        let tracks: Vec<Track> = params
            .playlist
            .tracks
            .iter()
            .map(|t| Track {
                title: t.display(),
                length_ms: t.seconds.saturating_mul(1000),
            })
            .collect();
        let index = params
            .playlist
            .current
            .clamp(0, (tracks.len() as i32 - 1).max(0));
        StillPlayer {
            status: match p.status {
                StatusParam::Playing => Status::Playing,
                StatusParam::Paused => Status::Paused,
                StatusParam::Stopped => Status::Stopped,
            },
            position_ms: (p.position * 1000.0).round() as i32,
            tracks,
            index,
            kbps: p.kbps,
            khz: p.khz,
            channels: p.channels,
            volume: (p.volume.clamp(0, 100) * 255 + 50) / 100,
            pan: (p.balance.clamp(-100, 100) * 127) / 100,
            shuffle: p.shuffle,
            repeat: p.repeat,
            crossfade: false,
            eq_enabled: params.equalizer.on,
            eq_preamp: eq_value(params.equalizer.preamp),
            eq_bands: params
                .equalizer
                .bands
                .iter()
                .map(|b| eq_value(*b))
                .collect(),
            spectrum: match params.vis.mode {
                VisModeParam::Analyzer => spectrum(&params.vis.bars, params.vis.peaks),
                _ => vec![0.0; BANDS],
            },
            scope: params.vis.mode == VisModeParam::Scope,
        }
    }

    fn current(&self) -> Option<&Track> {
        self.tracks.get(self.index as usize)
    }
}

impl Player for StillPlayer {
    fn status(&self) -> Status {
        self.status
    }

    fn position_ms(&self) -> i32 {
        match self.status {
            Status::Stopped => 0,
            _ => self.position_ms.min(self.length_ms()),
        }
    }

    fn length_ms(&self) -> i32 {
        self.current().map_or(0, |t| t.length_ms)
    }

    fn title(&self) -> String {
        self.current().map(|t| t.title.clone()).unwrap_or_default()
    }

    fn bitrate_kbps(&self) -> i32 {
        self.kbps
    }

    fn sample_rate_khz(&self) -> i32 {
        self.khz
    }

    fn channels(&self) -> i32 {
        self.channels
    }

    fn volume(&self) -> i32 {
        self.volume
    }

    fn set_volume(&mut self, v: i32) {
        self.volume = v;
    }

    fn shuffle(&self) -> bool {
        self.shuffle
    }

    fn set_shuffle(&mut self, on: bool) {
        self.shuffle = on;
    }

    fn repeat(&self) -> bool {
        self.repeat
    }

    fn set_repeat(&mut self, on: bool) {
        self.repeat = on;
    }

    fn crossfade(&self) -> bool {
        self.crossfade
    }

    fn set_crossfade(&mut self, on: bool) {
        self.crossfade = on;
    }

    fn eq_enabled(&self) -> bool {
        self.eq_enabled
    }

    fn set_eq_enabled(&mut self, on: bool) {
        self.eq_enabled = on;
    }

    fn eq_bands(&self) -> Vec<i32> {
        self.eq_bands.clone()
    }

    fn eq_preamp(&self) -> i32 {
        self.eq_preamp
    }

    fn set_eq_preamp(&mut self, v: i32) {
        self.eq_preamp = v;
    }

    fn pan(&self) -> i32 {
        self.pan
    }

    fn set_pan(&mut self, v: i32) {
        self.pan = v;
    }

    // the transport stands still: a skin's script that presses play on load changes nothing
    fn play(&mut self) {}
    fn pause(&mut self) {}
    fn stop(&mut self) {}
    fn next(&mut self) {}
    fn previous(&mut self) {}
    fn eject(&mut self) {}
    fn seek_to(&mut self, _ms: i32) {}

    fn set_eq_band(&mut self, band: i32, value: i32) {
        if let Some(b) = usize::try_from(band)
            .ok()
            .and_then(|i| self.eq_bands.get_mut(i))
        {
            *b = value;
        }
    }

    fn tracks(&self) -> Vec<Track> {
        self.tracks.clone()
    }

    fn track_index(&self) -> i32 {
        self.index
    }

    fn play_track(&mut self, _index: i32) {}

    fn spectrum(&self) -> Vec<f32> {
        if self.status == Status::Playing {
            self.spectrum.clone()
        } else {
            Vec::new()
        }
    }

    fn waveform(&self) -> Vec<f32> {
        if self.status != Status::Playing || !self.scope {
            return Vec::new();
        }
        // one slow sine across the window
        (0..576)
            .map(|i| 0.4 * (i as f32 * std::f32::consts::TAU / 576.0).sin())
            .collect()
    }

    fn vu(&self) -> (i32, i32) {
        if self.status == Status::Playing {
            (160, 150)
        } else {
            (0, 0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equalizer_sliders_map_onto_the_64_steps() {
        // the engine shows a band at step 63 - value * 64 / 256, 0 the top
        let step = |percent| 63 - eq_value(percent) * 64 / 256;
        assert_eq!(step(100), 0);
        assert_eq!(step(0), 63);
        assert_eq!(step(50), 31);
        assert_eq!(step(56), 28);
    }

    #[test]
    fn bars_fill_every_row_they_reach_into() {
        let levels = spectrum(&[11.75, 11.0625, 0.0, 1.5625], false);
        assert_eq!(levels.len(), BANDS);
        // twelve rows are fourteen sixteenths: two rows lie under the picture
        assert_eq!(levels[0], 14.0 / 15.0);
        assert_eq!(levels[3], 14.0 / 15.0);
        assert_eq!(levels[4], 14.0 / 15.0);
        assert_eq!(levels[8], 0.0);
        assert_eq!(levels[12], 4.0 / 15.0);
        assert_eq!(levels[16], 0.0);
        // with peak marks the mark is the top row
        assert_eq!(spectrum(&[12.0], true)[0], 13.0 / 15.0);
    }

    #[test]
    fn the_player_stands_still() {
        let mut player = StillPlayer::new(&Params::default());
        player.play_track(2);
        player.seek_to(0);
        player.stop();
        assert_eq!(player.status(), Status::Playing);
        assert_eq!(player.position_ms(), 3000);
        assert_eq!(player.track_index(), 0);
        assert_eq!(player.title(), "DJ Mike Llama - Llama Whipping Intro");
        assert_eq!(player.length_ms(), 5000);
        assert_eq!(player.volume(), 199);
    }
}
