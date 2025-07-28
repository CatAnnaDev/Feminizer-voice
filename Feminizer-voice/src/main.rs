use eframe::egui;
use egui_plot::{Line, Plot, PlotPoints};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

mod audio_processor;
use audio_processor::{AudioProcessor, FrequencyData};

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        ..Default::default()
    };

    eframe::run_native(
        "Feminizer voice",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(VoiceFrequencyApp::new()))
        }),
    )
}

struct VoiceFrequencyApp {
    audio_processor: Option<AudioProcessor>,
    is_recording: bool,
    frequency_history: VecDeque<f32>,
    amplitude_history: VecDeque<f32>,
    current_frequency: f32,
    current_amplitude: f32,
    frequency_data: Arc<Mutex<Option<FrequencyData>>>,
    error_message: Option<String>,
    min_amplitude_threshold: f32,
}

impl Default for VoiceFrequencyApp {
    fn default() -> Self {
        Self {
            audio_processor: None,
            is_recording: false,
            frequency_history: Default::default(),
            amplitude_history: Default::default(),
            current_frequency: 0.0,
            current_amplitude: 0.0,
            frequency_data: Arc::new(Mutex::new(None)),
            error_message: None,
            min_amplitude_threshold: 0.0200,
        }
    }
}

impl VoiceFrequencyApp {
    fn new() -> Self {
        Self {
            ..Default::default()
        }
    }

    fn start_recording(&mut self) {
        match AudioProcessor::new(self.frequency_data.clone()) {
            Ok(processor) => {
                self.audio_processor = Some(processor);
                self.is_recording = true;
                self.error_message = None;
                println!("Enregistrement démarré");
            }
            Err(e) => {
                self.error_message = Some(format!("Erreur audio: {}", e));
                println!("Erreur lors du démarrage: {}", e);
            }
        }
    }

    fn stop_recording(&mut self) {
        self.audio_processor = None;
        self.is_recording = false;
        println!("Enregistrement arrêté");
    }

    fn update_frequency_data(&mut self) -> bool {
        if let Ok(data_guard) = self.frequency_data.try_lock() {
            if let Some(data) = data_guard.as_ref() {
                if data.amplitude < self.min_amplitude_threshold {
                    return false;
                }

                let filtered_frequency =
                    if data.dominant_frequency >= 50.0 && data.dominant_frequency <= 450.0 {
                        data.dominant_frequency
                    } else {
                        0.0
                    };

                self.current_frequency = filtered_frequency;
                self.current_amplitude = data.amplitude;

                if filtered_frequency > 0.0 {
                    self.frequency_history.push_back(filtered_frequency);
                    self.amplitude_history.push_back(data.amplitude);
                } else {
                    self.frequency_history.push_back(0.0);
                    self.amplitude_history.push_back(0.0);
                }

                if self.frequency_history.len() > 100 {
                    self.frequency_history.pop_front();
                    self.amplitude_history.pop_front();
                }
                return true;
            }
        }
        false
    }

    fn frequency_to_note(&self, freq: f32) -> String {
        if freq < 50.0 || freq > 450.0 {
            return "Hors plage".to_string();
        }

        let notes = [
            (82.4, "E2"),
            (87.3, "F2"),
            (92.5, "F#2"),
            (98.0, "G2"),
            (103.8, "G#2"),
            (110.0, "A2"),
            (116.5, "A#2"),
            (123.5, "B2"),
            (130.8, "C3"),
            (138.6, "C#3"),
            (146.8, "D3"),
            (155.6, "D#3"),
            (164.8, "E3"),
            (174.6, "F3"),
            (185.0, "F#3"),
            (196.0, "G3"),
            (207.7, "G#3"),
            (220.0, "A3"),
            (233.1, "A#3"),
            (246.9, "B3"),
            (261.6, "C4"),
            (277.2, "C#4"),
            (293.7, "D4"),
            (311.1, "D#4"),
            (329.6, "E4"),
            (349.2, "F4"),
            (370.0, "F#4"),
            (392.0, "G4"),
            (415.3, "G#4"),
            (440.0, "A4"),
        ];

        let mut closest_note = "?";
        let mut min_diff = f32::INFINITY;

        for (note_freq, note_name) in &notes {
            let diff = (freq - note_freq).abs();
            if diff < min_diff {
                min_diff = diff;
                closest_note = note_name;
            }
        }

        format!("{} (~{:.1}Hz)", closest_note, freq)
    }
}

impl eframe::App for VoiceFrequencyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.update_frequency_data();

        egui::CentralPanel::default().show(ctx, |ui| {
            //ui.heading("🎤 Feminizer voice");
            //ui.separator();

            ui.horizontal(|ui| {
                if ui
                    .button(if self.is_recording {
                        "🛑 Arrêter"
                    } else {
                        "🎙️ Démarrer"
                    })
                    .clicked()
                {
                    if self.is_recording {
                        self.stop_recording();
                    } else {
                        self.start_recording();
                    }
                }

                ui.label(if self.is_recording {
                    "🔴 Enregistrement en cours..."
                } else {
                    "⚪ En attente"
                });

                ui.separator();
                ui.label("Seuil minimal:");
                ui.add(
                    egui::Slider::new(&mut self.min_amplitude_threshold, 0.001..=0.1)
                        .logarithmic(true)
                        .text("Amplitude"),
                );
            });

            if let Some(error) = &self.error_message {
                ui.colored_label(egui::Color32::RED, format!("❌ {}", error));
            }

            ui.separator();

            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label("Fréquence dominante:");
                    if self.current_frequency > 0.0
                        && self.current_frequency >= 50.0
                        && self.current_frequency <= 450.0
                    {
                        ui.colored_label(
                            egui::Color32::GREEN,
                            format!("{:.1} Hz", self.current_frequency),
                        );
                        ui.label(format!(
                            "Note: {}",
                            self.frequency_to_note(self.current_frequency)
                        ));
                    } else {
                        ui.colored_label(egui::Color32::GRAY, "Aucune fréquence détectée");
                    }
                });

                ui.separator();

                ui.vertical(|ui| {
                    ui.label("Amplitude:");
                    let amplitude_db = if self.current_amplitude > 0.0 {
                        20.0 * self.current_amplitude.log10()
                    } else {
                        -60.0
                    };
                    ui.label(format!("{:.1} dB", amplitude_db));

                    let level = ((amplitude_db + 60.0) / 60.0).clamp(0.0, 1.0);
                    let bar_color = if level > 0.8 {
                        egui::Color32::RED
                    } else if level > 0.4 {
                        egui::Color32::YELLOW
                    } else {
                        egui::Color32::GREEN
                    };

                    ui.add(
                        egui::ProgressBar::new(level)
                            .fill(bar_color)
                            .show_percentage(),
                    );
                });
            });

            ui.separator();

            if !self.frequency_history.is_empty() {
                ui.label("📈 Historique des fréquences:");

                ui.label("🔷 Fréquences graves (80-160 Hz):");
                let low_freq_points: PlotPoints = self
                    .frequency_history
                    .iter()
                    .enumerate()
                    .filter_map(|(i, &freq)| {
                        if freq >= 50.0 && freq <= 160.0 {
                            Some([i as f64, freq as f64])
                        } else {
                            None
                        }
                    })
                    .collect();

                Plot::new("low_frequency_plot")
                    .view_aspect(2.0)
                    .height(200.0)
                    .y_axis_label("Fréquence (Hz)")
                    .x_axis_label("Temps (échantillons)")
                    .include_y(50.0)// 80.0
                    .include_y(160.0)
                    .allow_zoom(false)
                    .allow_drag(false)
                    .show(ui, |plot_ui| {
                        if !low_freq_points.points().is_empty() {
                            plot_ui.line(
                                Line::new("", low_freq_points)
                                    .color(egui::Color32::LIGHT_BLUE)
                                    .width(2.0),
                            );
                        }

                        plot_ui.hline(
                            egui_plot::HLine::new("", 80.0)
                                .color(egui::Color32::BLUE)
                                .style(egui_plot::LineStyle::Solid)
                                .width(1.0),
                        );
                        plot_ui.hline(
                            egui_plot::HLine::new("", 160.0)
                                .color(egui::Color32::BLUE)
                                .style(egui_plot::LineStyle::Solid)
                                .width(1.0),
                        );
                    });

                ui.add_space(10.0);

                ui.label("🔸 Fréquences aiguës (180-310 Hz):");
                let high_freq_points: PlotPoints = self
                    .frequency_history
                    .iter()
                    .enumerate()
                    .filter_map(|(i, &freq)| {
                        if freq >= 180.0 && freq <= 500.0 {
                            Some([i as f64, freq as f64])
                        } else {
                            None
                        }
                    })
                    .collect();

                Plot::new("high_frequency_plot")
                    .view_aspect(2.0)
                    .height(200.0)
                    .y_axis_label("Fréquence (Hz)")
                    .x_axis_label("Temps (échantillons)")
                    .include_y(180.0)
                    .include_y(500.0) //310.0
                    .allow_zoom(false)
                    .allow_drag(false)
                    .show(ui, |plot_ui| {
                        if !high_freq_points.points().is_empty() {
                            plot_ui.line(
                                Line::new("", high_freq_points)
                                    .color(egui::Color32::from_rgb(255, 0, 255))
                                    .width(2.0),
                            );
                        }

                        plot_ui.hline(
                            egui_plot::HLine::new("", 180.0)
                                .color(egui::Color32::RED)
                                .style(egui_plot::LineStyle::Solid)
                                .width(1.0),
                        );
                        plot_ui.hline(
                            egui_plot::HLine::new("", 310.0)
                                .color(egui::Color32::RED)
                                .style(egui_plot::LineStyle::Solid)
                                .width(1.0),
                        );
                    });
            }

            ui.separator();
            ui.small("Plages: Graves 80-160 Hz (bleu) | Aiguës 180-310 Hz (rose)");
        });

        if self.is_recording {
            ctx.request_repaint();
        }
    }
}
