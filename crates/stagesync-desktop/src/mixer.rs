//! The mixer: the controls, the meters, and the state behind them.
//!
//! The panel is drawn under the waveform timeline, because that is where the user
//! looks when they ask "what will this sound like".
//!
//! Two rules govern what the panel shows:
//!
//! * **The reference is never a track.** It is the thing the other recordings were
//!   aligned to; it can be *monitored*, but mixing it into the output would double
//!   every recording. It does get a pan, because a recording that can be listened to
//!   can be placed in the field — but the pan changes what is heard and not what is
//!   written.
//! * **The level that is shown is a prediction, and says so.** The prediction is
//!   built from the measured peak of each recording, so it is an upper bound: it
//!   assumes every recording reaches its peak at the same instant. It is exactly
//!   the right thing to warn about clipping with.

use eframe::egui::{self, Rect, Sense, Stroke, StrokeKind, Vec2};
use stagesync_core::audio::PeakCache;
use stagesync_core::mixer::{db_to_linear, linear_to_db, MixSettings, MixTrack, MonitorMode};

use crate::theme::Palette;

/// What the panel needs to know about one recording.
#[derive(Debug, Clone)]
pub struct TrackInfo {
    /// The recording's name.
    pub name: String,
    /// The loudest sample of the recording, when it is known.
    pub peak: Option<f32>,
    /// Whether the recording has a reliable alignment.
    pub aligned: bool,
    /// How many channels the recording has.
    ///
    /// The pan law a mono recording is panned with is not the one a stereo
    /// recording is panned with, so the prediction needs to know which it is.
    pub channels: usize,
}

/// What the panel needs to know about the reference recording.
#[derive(Debug, Clone)]
pub struct ReferenceInfo {
    /// The reference recording's file name, when there is one.
    pub name: Option<String>,
    /// How many channels the reference has.
    pub channels: usize,
}

/// The state of the mixer.
#[derive(Debug, Clone)]
pub struct MixerState {
    /// One track per target recording, in the order of the target list.
    pub tracks: Vec<MixTrack>,
    /// The pan position of the reference recording: `-1` is hard left and `+1`
    /// is hard right.
    ///
    /// The reference is **monitored**, never mixed, so this control places the
    /// reference in the field while it is being listened to. It never changes
    /// the exported mix: the reference is not a track of the mix, and the
    /// reference soundtrack is never altered.
    pub reference_pan: f32,
    /// What is being listened to.
    pub mode: MonitorMode,
    /// The master fader, as a linear gain.
    pub master_gain: f32,
    /// The peak of the mix that was last written, when there was one.
    pub last_peak: Option<f32>,
    /// The gain reduction the last export's protection applied, in decibels.
    pub last_gain_reduction_db: f64,
    /// The loudest sample the mix that is playing has produced.
    pub live_peak: Option<f32>,
}

impl Default for MixerState {
    fn default() -> Self {
        Self {
            tracks: Vec::new(),
            reference_pan: 0.0,
            mode: MonitorMode::Both,
            master_gain: 1.0,
            last_peak: None,
            last_gain_reduction_db: 0.0,
            live_peak: None,
        }
    }
}

impl MixerState {
    /// A mixer with one track per recording.
    pub fn new(count: usize) -> Self {
        Self {
            tracks: (0..count).map(MixTrack::new).collect(),
            ..Self::default()
        }
    }

    /// Make the mixer match the number of recordings.
    ///
    /// A recording that is added gets a track of its own; a recording that is
    /// removed takes its track with it. The faders of the recordings that stay are
    /// left alone.
    pub fn sync_to(&mut self, count: usize) {
        while self.tracks.len() < count {
            let index = self.tracks.len();
            self.tracks.push(MixTrack::new(index));
        }
        self.tracks.truncate(count);
        for (i, track) in self.tracks.iter_mut().enumerate() {
            track.source_index = i;
        }
    }

    /// Remove the track for target `index` and re-index subsequent tracks.
    pub fn remove_target(&mut self, index: usize) {
        if index < self.tracks.len() {
            self.tracks.remove(index);
        }
        for (i, track) in self.tracks.iter_mut().enumerate() {
            track.source_index = i;
        }
    }

    /// The settings of the whole mix, for the exporter.
    pub fn settings(&self) -> MixSettings {
        MixSettings {
            tracks: self.tracks.clone(),
            master_gain: self.master_gain,
        }
    }

    /// The tracks the monitoring mode plays.
    ///
    /// The reference is never among them.
    pub fn monitored(&self) -> Vec<MixTrack> {
        MixSettings::for_mode(self.mode, &self.tracks)
    }

    /// The predicted level of the mix, and whether it would clip.
    ///
    /// The prediction is the sum of the recordings' measured peaks, each scaled by
    /// its own fader and by the gain that places it in the field. It is an upper
    /// bound — the recordings' peaks rarely fall at the same instant — and it is
    /// reported as a bound rather than as a measurement.
    pub fn predicted_output(&self, infos: &[TrackInfo]) -> (f32, f32) {
        let solo_active = self.tracks.iter().any(|t| t.enabled && t.solo);
        let mut left = 0f32;
        let mut right = 0f32;
        for (track, info) in self.tracks.iter().zip(infos.iter()) {
            if !info.aligned {
                continue;
            }
            let gain = track.effective_gain(solo_active);
            if gain == 0.0 {
                continue;
            }
            let Some(peak) = info.peak else {
                continue;
            };
            let matrix = stagesync_core::mixer::PanMatrix::for_source(
                info.channels.max(1),
                track.pan,
                track.pan_law,
                track.stereo_pan,
            );
            let gl = matrix.gain(0, 0).abs() + matrix.gain(0, 1).abs();
            let gr = matrix.gain(1, 0).abs() + matrix.gain(1, 1).abs();
            left += peak * gain * gl;
            right += peak * gain * gr;
        }
        let master = self.master_gain;
        (left * master, right * master)
    }
}

/// The loudest sample of a recording, from its peak cache.
pub fn peak_of(cache: &PeakCache) -> f32 {
    let mut peak = 0f32;
    for level in &cache.levels {
        for plane in level.maxs.iter().chain(level.mins.iter()) {
            for value in plane {
                peak = peak.max(value.abs());
            }
        }
    }
    peak
}

/// Draw the mixer panel.
///
/// The panel draws one row per recording: its name, its fader, its pan, its mute
/// and solo buttons and its meter. Above the rows are the monitoring mode, the
/// master fader and the predicted level of the whole mix.
///
/// The reference has a row of its own with a pan on it. It is **not** a track of
/// the mix — there is no "in mix" checkbox and no fader for it — but it is
/// monitored, and a recording that can be listened to can be placed in the
/// field.
pub fn panel(
    ui: &mut egui::Ui,
    state: &mut MixerState,
    infos: &[TrackInfo],
    reference: Option<&ReferenceInfo>,
) {
    let palette = Palette::for_mode(ui.visuals().dark_mode);
    ui.horizontal_wrapped(|ui| {
        egui::ComboBox::from_label("Monitor")
            .selected_text(state.mode.label())
            .show_ui(ui, |ui| {
                for mode in MonitorMode::ALL {
                    ui.selectable_value(&mut state.mode, mode, mode.label())
                        .on_hover_text(mode.description());
                }
            });
        ui.colored_label(palette.text_muted, state.mode.description());
    });

    // The master fader and the predicted upper-bound level of the aligned mix.
    let any_aligned = infos.iter().any(|i| i.aligned);
    let (left, right) = state.predicted_output(infos);
    let peak = left.max(right);
    ui.horizontal_wrapped(|ui| {
        let mut master_db = linear_to_db(state.master_gain);
        if ui
            .add(
                egui::Slider::new(&mut master_db, -60.0..=12.0)
                    .suffix(" dB")
                    .text("Master"),
            )
            .changed()
        {
            state.master_gain = db_to_linear(master_db);
        }
        if !any_aligned {
            ui.colored_label(
                palette.text_muted,
                "Predicted output (align tracks to preview)",
            );
        } else {
            ui.label(format!(
                "Predicted output (upper bound): {:.2} dBFS",
                linear_to_db(peak)
            ));
            if peak > 1.0 {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!(
                        "The mix may clip: upper bound {:.2} dBFS.",
                        linear_to_db(peak)
                    ),
                );
            } else if let (Some(written), reduction) =
                (state.last_peak, state.last_gain_reduction_db)
            {
                let mut text = format!("Last export {:.2} dBFS", linear_to_db(written));
                if reduction != 0.0 {
                    text.push_str(&format!(" (protection applied {:.2} dB)", -reduction));
                }
                ui.label(text);
            }
        }
    });

    // The level the mix that is playing has actually reached. The prediction above
    // is an upper bound taken from the recordings' own peaks; this is a
    // measurement of what came out of the mixer.
    if let Some(live) = state.live_peak {
        if live > 0.0 {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("Playing {:.2} dBFS", linear_to_db(live)));
                if live > 1.0 {
                    ui.colored_label(
                        ui.visuals().error_fg_color,
                        format!(
                            "The playing mix is above full scale: {:.2} dBFS.",
                            linear_to_db(live)
                        ),
                    );
                }
            });
        }
    }

    ui.separator();

    // The reference is monitored on its own, so it gets a row with a pan on it.
    // It is never a track of the mix, so it gets no "in mix" checkbox, no fader
    // and no solo button: the level it is heard at is the level it was recorded
    // at.
    if let Some(reference) = reference {
        reference_row(ui, state, reference, &palette);
    }

    if state.tracks.is_empty() {
        ui.label("There are no recordings to mix.");
        return;
    }

    let solo_active = state.tracks.iter().any(|t| t.enabled && t.solo);
    for (index, track) in state.tracks.iter_mut().enumerate() {
        let info = infos.get(index);
        let channels = info.map(|i| i.channels).unwrap_or(1);
        ui.push_id(index, |ui| {
            egui::Frame::new()
                .fill(palette.bg_surface)
                .stroke(Stroke::new(1.0, palette.border_subtle))
                .corner_radius(3.0)
                .inner_margin(egui::Margin::symmetric(8, 5))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut track.enabled, "In mix")
                            .on_hover_text("Include this recording in the mix");
                        let name = info
                            .map(|i| i.name.clone())
                            .unwrap_or_else(|| format!("Recording {}", index + 1));
                        ui.strong(name);
                        if let Some(info) = info {
                            if !info.aligned {
                                ui.colored_label(palette.warn, "no alignment: excluded from mix");
                            }
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let solo_btn = egui::Button::new(
                                egui::RichText::new("S").strong().color(if track.solo {
                                    palette.warn
                                } else {
                                    palette.text_primary
                                }),
                            )
                            .fill(if track.solo {
                                palette.warn.gamma_multiply(0.18)
                            } else {
                                palette.bg_panel
                            });
                            if ui
                                .add(solo_btn)
                                .on_hover_text("Solo: hear this recording on its own")
                                .clicked()
                            {
                                track.solo = !track.solo;
                            }
                            let mute_btn = egui::Button::new(
                                egui::RichText::new("M").strong().color(if track.mute {
                                    palette.error
                                } else {
                                    palette.text_primary
                                }),
                            )
                            .fill(if track.mute {
                                palette.error.gamma_multiply(0.18)
                            } else {
                                palette.bg_panel
                            });
                            if ui
                                .add(mute_btn)
                                .on_hover_text("Mute this recording")
                                .clicked()
                            {
                                track.mute = !track.mute;
                            }
                        });
                    });

                    ui.horizontal_wrapped(|ui| {
                        let mut gain_db = linear_to_db(track.gain);
                        if ui
                            .add(
                                egui::Slider::new(&mut gain_db, -60.0..=12.0)
                                    .suffix(" dB")
                                    .text("Level"),
                            )
                            .changed()
                        {
                            track.gain = db_to_linear(gain_db);
                        }
                        let mut trim_db = linear_to_db(track.trim);
                        if ui
                            .add(
                                egui::Slider::new(&mut trim_db, -24.0..=24.0)
                                    .suffix(" dB")
                                    .text("Trim"),
                            )
                            .changed()
                        {
                            track.trim = db_to_linear(trim_db);
                        }
                        let mut pan = track.pan;
                        if ui
                            .add(
                                egui::Slider::new(&mut pan, -1.0..=1.0)
                                    .suffix(" pan")
                                    .text("Pan"),
                            )
                            .changed()
                        {
                            track.pan = pan;
                        }
                        if channels <= 1 {
                            egui::ComboBox::from_label("Pan law")
                                .width(105.0)
                                .selected_text(match track.pan_law {
                                    stagesync_core::mixer::PanLaw::EqualPower => "Equal power",
                                    stagesync_core::mixer::PanLaw::ConstantGain => "Constant gain",
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut track.pan_law,
                                        stagesync_core::mixer::PanLaw::EqualPower,
                                        "Equal power",
                                    );
                                    ui.selectable_value(
                                        &mut track.pan_law,
                                        stagesync_core::mixer::PanLaw::ConstantGain,
                                        "Constant gain",
                                    );
                                });
                        } else {
                            egui::ComboBox::from_label("Stereo")
                                .width(105.0)
                                .selected_text(match track.stereo_pan {
                                    stagesync_core::mixer::StereoPan::Balance => "Balance",
                                    stagesync_core::mixer::StereoPan::StereoAware => "Stereo-aware",
                                    stagesync_core::mixer::StereoPan::MonoFold => "Mono fold",
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut track.stereo_pan,
                                        stagesync_core::mixer::StereoPan::Balance,
                                        "Balance",
                                    );
                                    ui.selectable_value(
                                        &mut track.stereo_pan,
                                        stagesync_core::mixer::StereoPan::StereoAware,
                                        "Stereo-aware",
                                    );
                                    ui.selectable_value(
                                        &mut track.stereo_pan,
                                        stagesync_core::mixer::StereoPan::MonoFold,
                                        "Mono fold",
                                    );
                                });
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.colored_label(palette.text_muted, "File peak");
                        let peak = info.and_then(|i| i.peak).unwrap_or(0.0);
                        let contribution = peak * track.effective_gain(solo_active);
                        meter(ui, contribution, &palette);
                        ui.monospace(format!("{:.1} dBFS", linear_to_db(contribution)));
                    });
                });
        });
    }
}

/// Draw the row of the reference recording.
///
/// The row carries a pan and nothing else. The reference is never a track of the
/// mix, so there is no fader to move and nothing to mute — the level it is
/// heard at is the level it was recorded at — but it *is* monitored, and a
/// recording that can be heard can be placed in the field.
fn reference_row(
    ui: &mut egui::Ui,
    state: &mut MixerState,
    info: &ReferenceInfo,
    palette: &Palette,
) {
    egui::Frame::new()
        .fill(palette.bg_surface)
        .stroke(Stroke::new(1.0, palette.border_subtle))
        .corner_radius(3.0)
        .inner_margin(egui::Margin::symmetric(8, 5))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Reference");
                if let Some(name) = &info.name {
                    ui.label(name.clone());
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.monospace(pan_label(state.reference_pan));
                });
            });

            ui.horizontal_wrapped(|ui| {
                let mut pan = state.reference_pan;
                let slider = ui
                    .add(
                        egui::Slider::new(&mut pan, -1.0..=1.0)
                            .suffix(" pan")
                            .text("Reference pan"),
                    )
                    .on_hover_text(
                        "Move the reference in the stereo field. At the centre it is heard \
                         exactly as it was recorded.",
                    );
                if slider.changed() {
                    state.reference_pan = pan;
                }
            });

            if info.channels > 2 {
                ui.colored_label(
                    palette.warn,
                    format!(
                        "This reference has {} channels, so it keeps all of them: the field \
                         has two sides.",
                        info.channels
                    ),
                );
            } else {
                ui.colored_label(
                    palette.text_muted,
                    "Monitored only: the reference is never part of the exported mix.",
                );
            }
        });
}

/// How a pan position is shown.
fn pan_label(pan: f32) -> String {
    let pan = pan.clamp(-1.0, 1.0);
    if pan.abs() < 0.005 {
        "Centre".to_string()
    } else if pan < 0.0 {
        format!("Left {:.0}%", -pan * 100.0)
    } else {
        format!("Right {:.0}%", pan * 100.0)
    }
}

/// Draw a level meter.
fn meter(ui: &mut egui::Ui, peak: f32, palette: &Palette) {
    let height = 10.0;
    let width = 128.0;
    let (rect, _response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, palette.bg_strip);
    painter.rect_stroke(
        rect,
        2.0,
        Stroke::new(1.0, palette.border_subtle),
        StrokeKind::Middle,
    );

    // The scale runs from -60 dBFS to 0 dBFS.
    let db = linear_to_db(peak);
    let fraction = ((db + 60.0) / 60.0).clamp(0.0, 1.0) as f32;
    if fraction > 0.0 {
        let filled = Rect::from_min_max(
            rect.left_top(),
            egui::pos2(rect.left() + rect.width() * fraction, rect.bottom()),
        );
        let colour = if db > -1.0 {
            palette.error
        } else if db > -12.0 {
            palette.warn
        } else {
            palette.ok
        };
        painter.rect_filled(filled, 2.0, colour);
    }
    // The 0 dBFS line.
    let zero = rect.right();
    painter.line_segment(
        [
            egui::pos2(zero, rect.top()),
            egui::pos2(zero, rect.bottom()),
        ],
        Stroke::new(1.0, palette.border_strong),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use stagesync_core::mixer::{PanLaw, StereoPan};

    #[test]
    fn the_mixer_keeps_a_track_for_every_recording() {
        let mut state = MixerState::new(2);
        assert_eq!(state.tracks.len(), 2);
        state.sync_to(3);
        assert_eq!(state.tracks.len(), 3);
        state.sync_to(1);
        assert_eq!(state.tracks.len(), 1);
    }

    #[test]
    fn a_muted_recording_contributes_nothing_to_the_prediction() {
        let mut state = MixerState::new(1);
        state.tracks[0].gain = 1.0;
        let infos = vec![TrackInfo {
            name: "a".to_string(),
            peak: Some(0.5),
            aligned: true,
            channels: 1,
        }];
        let (left, _) = state.predicted_output(&infos);
        assert!(left > 0.0, "the prediction was empty");
        state.tracks[0].mute = true;
        let (left, _) = state.predicted_output(&infos);
        assert!(left.abs() < 1e-9, "the muted recording was counted: {left}");
    }

    #[test]
    fn the_prediction_grows_with_the_fader() {
        let mut state = MixerState::new(1);
        let infos = vec![TrackInfo {
            name: "a".to_string(),
            peak: Some(0.5),
            aligned: true,
            channels: 1,
        }];
        state.tracks[0].gain = 1.0;
        let (quiet, _) = state.predicted_output(&infos);
        state.tracks[0].gain = 2.0;
        let (loud, _) = state.predicted_output(&infos);
        assert!(
            loud > quiet * 1.9,
            "the prediction did not follow the fader: {quiet} and {loud}"
        );
    }

    #[test]
    fn the_reference_is_never_a_track_of_the_mix() {
        let state = MixerState::new(2);
        let monitored = state.monitored();
        assert!(
            monitored.iter().all(|t| t.source_index < 2),
            "a track outside the recordings was monitored"
        );
    }

    #[test]
    fn the_pan_moves_the_prediction_towards_one_side() {
        let mut state = MixerState::new(1);
        state.tracks[0].pan_law = PanLaw::ConstantGain;
        state.tracks[0].stereo_pan = StereoPan::Balance;
        let infos = vec![TrackInfo {
            name: "a".to_string(),
            peak: Some(0.5),
            aligned: true,
            channels: 1,
        }];
        state.tracks[0].pan = -1.0;
        let (left, right) = state.predicted_output(&infos);
        assert!(
            left > right,
            "the recording was not panned left: {left} and {right}"
        );
    }

    #[test]
    fn the_reference_can_be_panned_without_becoming_a_track_of_the_mix() {
        let mut state = MixerState::new(2);
        state.reference_pan = -1.0;

        // The pan is the reference's own: it does not add a track to the mix, and
        // the settings the exporter reads do not carry it.
        let settings = state.settings();
        assert_eq!(
            settings.tracks.len(),
            2,
            "the reference became a track of the mix"
        );
        assert!(
            settings.tracks.iter().all(|t| t.source_index < 2),
            "a track outside the recordings was exported"
        );
        assert!(
            state.monitored().iter().all(|t| t.source_index < 2),
            "a track outside the recordings was monitored"
        );
        assert!(
            (state.reference_pan + 1.0).abs() < 1e-9,
            "the reference pan was not kept: {}",
            state.reference_pan
        );
    }

    #[test]
    fn unaligned_tracks_are_excluded_from_predicted_output() {
        let mut state = MixerState::new(2);
        state.tracks[0].gain = 2.0;
        state.tracks[1].gain = 2.0;
        let infos = vec![
            TrackInfo {
                name: "mic1.wav".to_string(),
                peak: Some(0.9),
                aligned: false,
                channels: 1,
            },
            TrackInfo {
                name: "mic2.wav".to_string(),
                peak: Some(0.9),
                aligned: false,
                channels: 1,
            },
        ];
        let (l, r) = state.predicted_output(&infos);
        assert!(
            l == 0.0 && r == 0.0,
            "unaligned tracks predicted {l}/{r} instead of 0.0"
        );
    }
}
