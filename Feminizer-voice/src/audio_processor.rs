use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Stream, StreamConfig};
use rustfft::{FftPlanner, num_complex::Complex};
use std::sync::{Arc, Mutex};

pub struct FrequencyData {
    pub dominant_frequency: f32,
    pub amplitude: f32,
}

pub struct AudioProcessor {
    _stream: Stream,
}

impl AudioProcessor {
    pub fn new(frequency_data: Arc<Mutex<Option<FrequencyData>>>) -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow::anyhow!("Aucun périphérique d'entrée audio trouvé"))?;

        let config = device.default_input_config()?;
        let sample_rate = config.sample_rate().0 as f32;
        let channels = config.channels() as usize;

        println!(
            "Configuration audio: {} Hz, {} canaux",
            sample_rate, channels
        );

        let stream_config = StreamConfig {
            channels: config.channels(),
            sample_rate: config.sample_rate(),
            buffer_size: cpal::BufferSize::Fixed(1024),
        };

        let processor = FrequencyProcessor::new(sample_rate, 1024);
        let processor = Arc::new(Mutex::new(processor));

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                Self::build_stream::<f32>(&device, &stream_config, processor, frequency_data)?
            }
            cpal::SampleFormat::I16 => {
                Self::build_stream::<i16>(&device, &stream_config, processor, frequency_data)?
            }
            cpal::SampleFormat::U16 => {
                Self::build_stream::<u16>(&device, &stream_config, processor, frequency_data)?
            }
            format => return Err(anyhow::anyhow!("Format audio non supporté: {:?}", format)),
        };

        stream.play()?;

        Ok(AudioProcessor { _stream: stream })
    }

    fn build_stream<T>(
        device: &Device,
        config: &StreamConfig,
        processor: Arc<Mutex<FrequencyProcessor>>,
        frequency_data: Arc<Mutex<Option<FrequencyData>>>,
    ) -> Result<Stream>
    where
        T: cpal::Sample + cpal::SizedSample + Send + 'static,
        f32: cpal::FromSample<T>,
    {
        let channels = config.channels as usize;

        let stream = device.build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                let samples: Vec<f32> = if channels == 1 {
                    data.iter()
                        .map(|&s| cpal::Sample::to_sample::<f32>(s))
                        .collect()
                } else {
                    data.chunks(channels)
                        .map(|chunk| {
                            let sum: f32 = chunk
                                .iter()
                                .map(|&s| cpal::Sample::to_sample::<f32>(s))
                                .sum();
                            sum / channels as f32
                        })
                        .collect()
                };

                if let Ok(mut proc) = processor.try_lock() {
                    if let Some(result) = proc.process_samples(&samples) {
                        if let Ok(mut data_guard) = frequency_data.try_lock() {
                            *data_guard = Some(result);
                        }
                    }
                }
            },
            |err| eprintln!("Erreur du stream audio: {}", err),
            None,
        )?;

        Ok(stream)
    }
}

struct FrequencyProcessor {
    sample_rate: f32,
    buffer_size: usize,
    buffer: Vec<f32>,
    window: Vec<f32>,
    fft_planner: FftPlanner<f32>,
    buffer_pos: usize,
}

impl FrequencyProcessor {
    fn new(sample_rate: f32, buffer_size: usize) -> Self {
        let window: Vec<f32> = (0..buffer_size)
            .map(|i| {
                let angle = 2.0 * std::f32::consts::PI * i as f32 / (buffer_size - 1) as f32;
                0.5 * (1.0 - angle.cos())
            })
            .collect();

        Self {
            sample_rate,
            buffer_size,
            buffer: vec![0.0; buffer_size],
            window,
            fft_planner: FftPlanner::new(),
            buffer_pos: 0,
        }
    }

    fn process_samples(&mut self, samples: &[f32]) -> Option<FrequencyData> {
        for &sample in samples {
            self.buffer[self.buffer_pos] = sample;
            self.buffer_pos = (self.buffer_pos + 1) % self.buffer_size;

            if self.buffer_pos == 0 {
                return Some(self.analyze_frequency());
            }
        }
        None
    }

    fn analyze_frequency(&mut self) -> FrequencyData {
        let windowed: Vec<Complex<f32>> = self
            .buffer
            .iter()
            .zip(self.window.iter())
            .map(|(&sample, &window_val)| Complex::new(sample * window_val, 0.0))
            .collect();

        let mut fft_input = windowed;
        let fft = self.fft_planner.plan_fft_forward(self.buffer_size);
        fft.process(&mut fft_input);

        let spectrum: Vec<f32> = fft_input[..self.buffer_size / 2]
            .iter()
            .map(|c| c.norm())
            .collect();

        let min_bin = (50.0 * self.buffer_size as f32 / self.sample_rate) as usize;
        let max_bin = (450.0 * self.buffer_size as f32 / self.sample_rate) as usize;
        let max_bin = max_bin.min(spectrum.len() - 1);

        let mut max_magnitude = 0.0f32;
        let mut dominant_bin = 0;

        for i in min_bin..=max_bin {
            if spectrum[i] > max_magnitude {
                max_magnitude = spectrum[i];
                dominant_bin = i;
            }
        }

        let dominant_frequency = if dominant_bin > 0 && dominant_bin < spectrum.len() - 1 {
            let y1 = spectrum[dominant_bin - 1];
            let y2 = spectrum[dominant_bin];
            let y3 = spectrum[dominant_bin + 1];

            let a = (y1 - 2.0 * y2 + y3) / 2.0;
            let b = (y3 - y1) / 2.0;

            let x_offset = if a != 0.0 { -b / (2.0 * a) } else { 0.0 };
            let bin_frequency = dominant_bin as f32 * self.sample_rate / self.buffer_size as f32;
            let frequency_resolution = self.sample_rate / self.buffer_size as f32;

            bin_frequency + x_offset * frequency_resolution
        } else {
            dominant_bin as f32 * self.sample_rate / self.buffer_size as f32
        };

        let rms: f32 = self.buffer.iter().map(|&x| x * x).sum::<f32>() / self.buffer.len() as f32;
        let amplitude = rms.sqrt();

        FrequencyData {
            dominant_frequency: if max_magnitude > 0.001 {
                dominant_frequency
            } else {
                0.0
            },
            amplitude,
        }
    }
}
