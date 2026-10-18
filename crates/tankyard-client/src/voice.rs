use crate::ui::*;
use cpal::{
    FromSample, Sample, SizedSample, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use macroquad::prelude::*;
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, Instant},
};
use tankyard_protocol::*;
use tankyard_server::NetworkClient;

const MAX_SPEAKERS: usize = 16;
const MAX_BUFFER: usize = VOICE_FRAME_SAMPLES * 10;

pub struct VoiceChat {
    muted: bool,
    disabled: bool,
    active: bool,
    session: Option<(u64, u32)>,
    sequence: u32,
    input: Option<Stream>,
    output: Option<Stream>,
    captured: Option<Receiver<[f32; VOICE_FRAME_SAMPLES]>>,
    encoder: Option<opus::Encoder>,
    decoders: HashMap<u64, VoiceDecoder>,
    capture_enabled: Arc<AtomicBool>,
    mixer: Arc<Mutex<Mixer>>,
    errors: Receiver<String>,
    report_error: SyncSender<String>,
    error: Option<String>,
    output_failed: bool,
    speaking: Option<Instant>,
    microphone: Texture2D,
    microphone_off: Texture2D,
}

impl VoiceChat {
    pub fn new(disabled: bool) -> Self {
        let (report_error, errors) = mpsc::sync_channel(4);
        let microphone = Texture2D::from_file_with_format(
            include_bytes!("../../../assets/icons/lucide/mic.png"),
            Some(ImageFormat::Png),
        );
        let microphone_off = Texture2D::from_file_with_format(
            include_bytes!("../../../assets/icons/lucide/mic-off.png"),
            Some(ImageFormat::Png),
        );
        microphone.set_filter(FilterMode::Linear);
        microphone_off.set_filter(FilterMode::Linear);
        Self {
            muted: true,
            disabled,
            active: false,
            session: None,
            sequence: 0,
            input: None,
            output: None,
            captured: None,
            encoder: None,
            decoders: HashMap::new(),
            capture_enabled: Arc::new(AtomicBool::new(false)),
            mixer: Arc::new(Mutex::new(Mixer::default())),
            errors,
            report_error,
            error: None,
            output_failed: false,
            speaking: None,
            microphone,
            microphone_off,
        }
    }

    pub fn reset(&mut self) {
        self.mute();
        self.output = None;
        self.session = None;
        self.active = false;
        self.sequence = 0;
        self.output_failed = false;
        self.error = None;
        self.decoders.clear();
        if let Ok(mut mixer) = self.mixer.lock() {
            *mixer = Mixer::default();
        }
        while self.errors.try_recv().is_ok() {}
    }

    fn mute(&mut self) {
        self.capture_enabled.store(false, Ordering::Relaxed);
        self.input = None;
        self.captured = None;
        self.encoder = None;
        self.muted = true;
        self.speaking = None;
    }

    pub fn toggle(&mut self) {
        if !self.muted {
            self.mute();
            return;
        }
        if self.disabled || !self.active {
            return;
        }
        self.error = None;
        self.output_failed = false;
        match self.start_input() {
            Ok(()) => self.muted = false,
            Err(error) => {
                self.mute();
                self.error = Some(format!("Microphone unavailable: {error}"));
            }
        }
    }

    pub fn update(&mut self, client: &mut NetworkClient, playing: bool, volume: f32) {
        let session = client.player_round.map(|round| (client.client_id(), round));
        if session != self.session {
            self.reset();
            self.session = session;
        }
        self.active = !self.disabled
            && client.is_ready()
            && client.voice_enabled
            && !client.voice_muted
            && playing;
        while let Ok(error) = self.errors.try_recv() {
            self.mute();
            self.output = None;
            self.output_failed = true;
            self.error = Some(error);
        }
        if !client.voice_enabled || client.voice_muted || !client.is_ready() {
            self.mute();
        }
        self.capture_enabled
            .store(self.active && !self.muted, Ordering::Relaxed);
        if let Some(captured) = &self.captured {
            // Send at most the latest four frames after a render stall.
            let mut latest = VecDeque::with_capacity(4);
            while let Ok(frame) = captured.try_recv() {
                if latest.len() == 4 {
                    latest.pop_front();
                }
                latest.push_back(frame);
            }
            if self.active && !self.muted {
                for samples in latest {
                    let Some(encoder) = &mut self.encoder else {
                        break;
                    };
                    let mut encoded = vec![0; MAX_VOICE_PAYLOAD];
                    let Ok(length) = encoder.encode_float(&samples, &mut encoded) else {
                        continue;
                    };
                    encoded.truncate(length);
                    self.sequence = self.sequence.wrapping_add(1);
                    let frame = VoiceFrame {
                        round: client.player_round.unwrap_or(0),
                        sequence: self.sequence,
                        samples: encoded,
                    };
                    if client.send_voice(frame) {
                        self.speaking = Some(Instant::now());
                    }
                }
            }
        }
        // Muting a microphone never disables hearing nearby players. Playback
        // is unavailable while dead, paused, disconnected, or audio is disabled.
        let listening = !self.disabled && client.voice_enabled && client.is_ready() && playing;
        if !listening {
            client.voice_packets.clear();
            self.decoders.clear();
            if let Ok(mut mixer) = self.mixer.lock() {
                mixer.voices.clear();
                mixer.a = 0.;
                mixer.b = 0.;
            }
            return;
        }
        if !client.voice_packets.is_empty()
            && self.output.is_none()
            && !self.output_failed
            && let Err(error) = self.start_output()
        {
            self.error = Some(format!("Voice playback unavailable: {error}"));
            self.output_failed = true;
        }
        self.decoders
            .retain(|_, voice| voice.received.elapsed() < Duration::from_secs(1));
        let mut decoded = Vec::with_capacity(client.voice_packets.len());
        while let Some(packet) = client.voice_packets.pop_front() {
            if self.decoders.len() >= MAX_SPEAKERS && !self.decoders.contains_key(&packet.player) {
                continue;
            }
            if let std::collections::hash_map::Entry::Vacant(entry) =
                self.decoders.entry(packet.player)
            {
                let Ok(voice) = VoiceDecoder::new() else {
                    continue;
                };
                entry.insert(voice);
            }
            if let Some(samples) = self
                .decoders
                .get_mut(&packet.player)
                .and_then(|voice| voice.decode(&packet))
            {
                decoded.push((packet.player, samples));
            }
        }
        // Codec work runs on the game thread, outside the audio callback lock.
        if let Ok(mut mixer) = self.mixer.lock() {
            mixer.volume = volume.clamp(0., 1.);
            mixer
                .voices
                .retain(|_, voice| voice.received.elapsed() < Duration::from_secs(1));
            for (player, samples) in decoded {
                mixer.voices.entry(player).or_default().push(samples);
            }
        }
    }

    fn start_input(&mut self) -> Result<(), String> {
        self.encoder = Some(voice_encoder().map_err(|e| e.to_string())?);
        let device = cpal::default_host()
            .default_input_device()
            .ok_or("No input device")?;
        let supported = device.default_input_config().map_err(|e| e.to_string())?;
        let config: StreamConfig = supported.clone().into();
        let (sender, receiver) = mpsc::sync_channel(8);
        let make = |format| match format {
            cpal::SampleFormat::F32 => self.input_stream::<f32>(&device, &config, sender),
            cpal::SampleFormat::I16 => self.input_stream::<i16>(&device, &config, sender),
            cpal::SampleFormat::U16 => self.input_stream::<u16>(&device, &config, sender),
            cpal::SampleFormat::I32 => self.input_stream::<i32>(&device, &config, sender),
            _ => Err("Unsupported microphone sample format".into()),
        };
        let stream = make(supported.sample_format())?;
        stream.play().map_err(|e| e.to_string())?;
        self.captured = Some(receiver);
        self.input = Some(stream);
        Ok(())
    }

    fn input_stream<T>(
        &self,
        device: &cpal::Device,
        config: &StreamConfig,
        sender: SyncSender<[f32; VOICE_FRAME_SAMPLES]>,
    ) -> Result<Stream, String>
    where
        T: SizedSample,
        f32: FromSample<T>,
    {
        let enabled = self.capture_enabled.clone();
        let errors = self.report_error.clone();
        let channels = usize::from(config.channels);
        let mut capture = Capture::new(config.sample_rate.0);
        device
            .build_input_stream(
                config,
                move |data: &[T], _| {
                    if !enabled.load(Ordering::Relaxed) {
                        capture.reset();
                        return;
                    }
                    for frame in data.chunks_exact(channels) {
                        let mono = frame
                            .iter()
                            .map(|sample| f32::from_sample(*sample))
                            .sum::<f32>()
                            / channels as f32;
                        capture.push(mono, &sender);
                    }
                },
                move |error| {
                    let _ = errors.try_send(format!("Microphone disconnected: {error}"));
                },
                None,
            )
            .map_err(|e| e.to_string())
    }

    fn start_output(&mut self) -> Result<(), String> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or("No output device")?;
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let config: StreamConfig = supported.clone().into();
        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => self.output_stream::<f32>(&device, &config),
            cpal::SampleFormat::I16 => self.output_stream::<i16>(&device, &config),
            cpal::SampleFormat::U16 => self.output_stream::<u16>(&device, &config),
            cpal::SampleFormat::I32 => self.output_stream::<i32>(&device, &config),
            _ => Err("Unsupported speaker sample format".into()),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        self.output = Some(stream);
        Ok(())
    }

    pub fn effects_volume(&mut self, volume: f32) {
        if let Ok(mut mixer) = self.mixer.lock() {
            mixer.alarm_volume = volume.clamp(0., 1.);
        }
    }

    pub fn boss_alarm(&mut self, volume: f32) {
        if self.disabled || volume <= 0. {
            return;
        }
        if self.output.is_none()
            && let Err(error) = self.start_output()
        {
            self.error = Some(format!("Audio unavailable: {error}"));
            return;
        }
        if let Ok(mut mixer) = self.mixer.lock() {
            let wav = include_bytes!("../../../assets/audio/boss-arrival.wav");
            mixer.alarm.clear();
            mixer.alarm.extend(
                wav[44..]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|bytes| f32::from(i16::from_le_bytes([bytes[0], bytes[1]])) / 32768.),
            );
            mixer.alarm_volume = volume.clamp(0., 1.);
        }
    }

    fn output_stream<T>(
        &self,
        device: &cpal::Device,
        config: &StreamConfig,
    ) -> Result<Stream, String>
    where
        T: SizedSample + FromSample<f32>,
    {
        let mixer = self.mixer.clone();
        let errors = self.report_error.clone();
        let channels = usize::from(config.channels);
        let rate = config.sample_rate.0;
        device
            .build_output_stream(
                config,
                move |data: &mut [T], _| {
                    if let Ok(mut mixer) = mixer.try_lock() {
                        for frame in data.chunks_exact_mut(channels) {
                            let sample = T::from_sample(mixer.sample(rate));
                            frame.fill(sample);
                        }
                    } else {
                        data.fill(T::EQUILIBRIUM);
                    }
                },
                move |error| {
                    let _ = errors.try_send(format!("Audio output disconnected: {error}"));
                },
                None,
            )
            .map_err(|e| e.to_string())
    }

    pub fn rect(s: f32) -> Rect {
        Rect::new(
            screen_width() - 206. * s,
            screen_height() - 62. * s,
            184. * s,
            40. * s,
        )
    }

    pub fn captures_pointer(&self, s: f32) -> bool {
        contains(Self::rect(s))
    }

    pub fn draw(&mut self, s: f32, online: bool, server_muted: bool) {
        let r = Self::rect(s);
        let hovered = contains(r);
        let transmitting = self.active && !self.muted;
        let speaking = transmitting
            && self
                .speaking
                .is_some_and(|time| time.elapsed() < Duration::from_millis(180));
        panel(r);
        let color = if speaking {
            GREEN
        } else if transmitting {
            CYAN
        } else {
            muted_color()
        };
        let icon = if transmitting {
            &self.microphone
        } else {
            &self.microphone_off
        };
        draw_texture_ex(
            icon,
            r.x + r.w - 31. * s,
            r.y + 8. * s,
            color,
            DrawTextureParams {
                dest_size: Some(vec2(24. * s, 24. * s)),
                ..Default::default()
            },
        );
        let title = if self.disabled {
            "Audio disabled"
        } else if !online {
            "Voice offline"
        } else if server_muted {
            "Server muted"
        } else if self.error.is_some() {
            "Audio unavailable"
        } else if !self.active {
            "Voice paused"
        } else if transmitting {
            "Mic on [M]"
        } else {
            "Mic muted [M]"
        };
        label(title, r.x + 10. * s, r.y + 25. * s, 13. * s, color);
        if hovered && is_mouse_button_pressed(MouseButton::Left) {
            self.toggle();
        }
        if hovered {
            let text = self.error.as_deref().unwrap_or(if self.disabled {
                "Audio is disabled for this session."
            } else if !online {
                "Proximity voice is available in online arenas."
            } else if server_muted {
                "A server moderator muted your microphone."
            } else if !self.active {
                "Voice resumes when you are alive and playing."
            } else if transmitting {
                "Click or M to mute. Nearby players can hear you."
            } else {
                "Click or M to unmute. Use headphones for voice chat."
            });
            let help = Rect::new(
                r.x - 124. * s,
                screen_height() - 292. * s,
                308. * s,
                66. * s,
            );
            panel(help);
            wrap(
                text,
                help.x + 10. * s,
                help.y + 22. * s,
                help.w - 20. * s,
                13. * s,
                text_color(),
            );
        }
    }
}

fn voice_encoder() -> Result<opus::Encoder, opus::Error> {
    let mut encoder = opus::Encoder::new(
        VOICE_SAMPLE_RATE,
        opus::Channels::Mono,
        opus::Application::Voip,
    )?;
    encoder.set_bitrate(opus::Bitrate::Bits(VOICE_BITRATE))?;
    encoder.set_max_bandwidth(opus::Bandwidth::Fullband)?;
    encoder.set_complexity(10)?;
    encoder.set_vbr(true)?;
    encoder.set_vbr_constraint(true)?;
    encoder.set_inband_fec(true)?;
    encoder.set_packet_loss_perc(10)?;
    Ok(encoder)
}

struct VoiceDecoder {
    codec: opus::Decoder,
    sequence: Option<u32>,
    received: Instant,
    volume: f32,
}
impl VoiceDecoder {
    fn new() -> Result<Self, opus::Error> {
        Ok(Self {
            codec: opus::Decoder::new(VOICE_SAMPLE_RATE, opus::Channels::Mono)?,
            sequence: None,
            received: Instant::now(),
            volume: 0.,
        })
    }
    fn decode(&mut self, packet: &VoicePacket) -> Option<Vec<f32>> {
        if !packet.frame.valid()
            || self
                .sequence
                .is_some_and(|last| !sequence_newer(packet.frame.sequence, last))
            || self.codec.get_nb_samples(&packet.frame.samples).ok()? != VOICE_FRAME_SAMPLES
        {
            return None;
        }
        let missing = self.sequence.map_or(0, |last| {
            packet.frame.sequence.wrapping_sub(last).saturating_sub(1)
        });
        let discontinuity = missing > 3 || self.received.elapsed() > Duration::from_millis(300);
        if discontinuity {
            self.codec.reset_state().ok()?;
        }
        let missing = if discontinuity { 0 } else { missing as usize };
        let mut samples = vec![0.; (missing + 1) * VOICE_FRAME_SAMPLES];
        for index in 0..missing {
            // The most recent lost frame can use in-band FEC. Earlier losses
            // use Opus's waveform concealment rather than a hard silence gap.
            let fec = index + 1 == missing;
            let input = if fec {
                packet.frame.samples.as_slice()
            } else {
                &[]
            };
            self.codec
                .decode_float(
                    input,
                    &mut samples[index * VOICE_FRAME_SAMPLES..(index + 1) * VOICE_FRAME_SAMPLES],
                    fec,
                )
                .ok()?;
        }
        self.codec
            .decode_float(
                &packet.frame.samples,
                &mut samples[missing * VOICE_FRAME_SAMPLES..],
                false,
            )
            .ok()?;
        let gain = f32::from(packet.volume) / 255.;
        let start = if self.sequence.is_none() || discontinuity {
            gain
        } else {
            self.volume
        };
        let length = samples.len() as f32;
        for (index, sample) in samples.iter_mut().enumerate() {
            *sample *= start + (gain - start) * (index + 1) as f32 / length;
        }
        self.sequence = Some(packet.frame.sequence);
        self.received = Instant::now();
        self.volume = gain;
        Some(samples)
    }
}

struct Capture {
    rate: u32,
    phase: u32,
    sum: f32,
    count: u32,
    frame: [f32; VOICE_FRAME_SAMPLES],
    previous: f32,
    index: usize,
    energy: f32,
    hangover: u8,
}

impl Capture {
    fn new(rate: u32) -> Self {
        Self {
            rate: rate.max(1),
            phase: 0,
            sum: 0.,
            count: 0,
            frame: [0.; VOICE_FRAME_SAMPLES],
            previous: 0.,
            index: 0,
            energy: 0.,
            hangover: 0,
        }
    }
    fn reset(&mut self) {
        self.phase = 0;
        self.sum = 0.;
        self.count = 0;
        self.index = 0;
        self.energy = 0.;
        self.hangover = 0;
        self.previous = 0.;
    }
    fn push(&mut self, sample: f32, sender: &SyncSender<[f32; VOICE_FRAME_SAMPLES]>) {
        let sample = if sample.is_finite() {
            sample.clamp(-1., 1.)
        } else {
            0.
        };
        self.phase += VOICE_SAMPLE_RATE;
        self.sum += sample;
        self.count += 1;
        while self.phase >= self.rate {
            self.phase -= self.rate;
            let mono = if self.rate > VOICE_SAMPLE_RATE && self.count > 0 {
                self.sum / self.count as f32
            } else {
                self.previous
                    + (sample - self.previous) * (1. - self.phase as f32 / VOICE_SAMPLE_RATE as f32)
            };
            self.sum = 0.;
            self.count = 0;
            self.energy += mono * mono;
            self.frame[self.index] = mono;
            self.index += 1;
            if self.index == VOICE_FRAME_SAMPLES {
                if self.energy / VOICE_FRAME_SAMPLES as f32 > 0.000009 {
                    self.hangover = 15;
                }
                if self.hangover > 0 {
                    let _ = sender.try_send(self.frame);
                    self.hangover -= 1;
                }
                self.index = 0;
                self.energy = 0.;
            }
        }
        self.previous = sample;
    }
}

struct Playback {
    queue: VecDeque<f32>,
    started: bool,
    received: Instant,
}
impl Default for Playback {
    fn default() -> Self {
        Self {
            queue: VecDeque::with_capacity(MAX_BUFFER),
            started: false,
            received: Instant::now(),
        }
    }
}
impl Playback {
    fn push(&mut self, samples: Vec<f32>) {
        if self.queue.len() + samples.len() > MAX_BUFFER {
            self.queue.clear();
            self.started = false;
        }
        self.queue.extend(samples.into_iter().take(MAX_BUFFER));
        self.received = Instant::now();
    }
    fn sample(&mut self) -> f32 {
        if !self.started && self.queue.len() >= VOICE_FRAME_SAMPLES * 2 {
            self.started = true;
        }
        if !self.started {
            return 0.;
        }
        self.queue.pop_front().unwrap_or_else(|| {
            self.started = false;
            0.
        })
    }
}

#[derive(Default)]
struct Mixer {
    voices: HashMap<u64, Playback>,
    phase: f64,
    a: f32,
    b: f32,
    volume: f32,
    alarm: VecDeque<f32>,
    alarm_volume: f32,
}
impl Mixer {
    fn sample(&mut self, rate: u32) -> f32 {
        while self.phase >= 1. {
            self.phase -= 1.;
            self.a = self.b;
            self.b = self.voices.values_mut().map(Playback::sample).sum::<f32>() * self.volume
                + self.alarm.pop_front().unwrap_or(0.) * self.alarm_volume;
        }
        let sample = self.a + (self.b - self.a) * self.phase as f32;
        self.phase += f64::from(VOICE_SAMPLE_RATE) / f64::from(rate.max(1));
        if self.volume == 0. && self.alarm.is_empty() {
            0.
        } else {
            sample.clamp(-1., 1.)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_resamples_common_devices_and_never_sends_silence() {
        for rate in [8_000, 16_000, 44_100, 48_000, 96_000] {
            let (tx, rx) = mpsc::sync_channel(64);
            let mut capture = Capture::new(rate);
            for _ in 0..rate {
                capture.push(0., &tx);
            }
            assert!(rx.try_recv().is_err());
            for index in 0..rate {
                capture.push(
                    (index as f32 / rate as f32 * 440. * std::f32::consts::TAU).sin() * 0.5,
                    &tx,
                );
            }
            assert_eq!(
                rx.try_iter().count(),
                50,
                "{rate} Hz capture must yield 50 frames/second"
            );
            capture.reset();
            assert_eq!(capture.index, 0);
            assert_eq!(capture.hangover, 0);
        }
    }

    fn packet(sequence: u32) -> VoicePacket {
        let mut encoder = voice_encoder().unwrap();
        let signal: Vec<_> = (0..VOICE_FRAME_SAMPLES)
            .map(|i| {
                (i as f32 * 440. * std::f32::consts::TAU / VOICE_SAMPLE_RATE as f32).sin() * 0.5
            })
            .collect();
        let mut samples = vec![0; MAX_VOICE_PAYLOAD];
        let length = encoder.encode_float(&signal, &mut samples).unwrap();
        samples.truncate(length);
        VoicePacket {
            player: 1,
            volume: 255,
            frame: VoiceFrame {
                round: 1,
                sequence,
                samples,
            },
        }
    }

    #[test]
    fn opus_decodes_fullband_audio_conceals_loss_and_rejects_replays() {
        let mut decoder = VoiceDecoder::new().unwrap();
        let samples = decoder.decode(&packet(u32::MAX)).unwrap();
        assert_eq!(samples.len(), VOICE_FRAME_SAMPLES);
        assert!(samples.iter().all(|v| v.is_finite() && v.abs() <= 1.));
        assert!(samples.iter().any(|v| v.abs() > 0.2));
        assert_eq!(
            decoder.decode(&packet(0)).unwrap().len(),
            VOICE_FRAME_SAMPLES
        );
        assert!(decoder.decode(&packet(u32::MAX)).is_none());
        assert!(decoder.decode(&packet(0)).is_none());
        assert_eq!(
            decoder.decode(&packet(3)).unwrap().len(),
            VOICE_FRAME_SAMPLES * 3
        );
        let mut malformed = packet(4);
        malformed.frame.samples = vec![0];
        assert!(decoder.decode(&malformed).is_none());
        assert!(decoder.decode(&packet(4)).is_some());
    }

    #[test]
    fn playback_bounds_stalled_audio_and_mixer_obeys_master_mute() {
        let mut mixer = Mixer {
            volume: 0.5,
            ..Default::default()
        };
        let voice = mixer.voices.entry(1).or_default();
        voice.push(vec![0.5; VOICE_FRAME_SAMPLES * 2]);
        for _ in 0..480 {
            assert!((0. ..=0.25).contains(&mixer.sample(48_000)));
        }
        mixer.volume = 0.;
        assert_eq!(mixer.sample(48_000), 0.);
        let voice = mixer.voices.entry(1).or_default();
        for _ in 0..500 {
            voice.push(vec![0.5; VOICE_FRAME_SAMPLES]);
        }
        assert!(voice.queue.len() <= MAX_BUFFER);
    }
}
