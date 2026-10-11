//! Native WASAPI capture/playback. The WebView never owns the microphone.
//! Callbacks only copy/downmix frames or read the bounded playback queue.
//! WebRTC AEC/NS/VAD and endpointing run on a dedicated worker thread.
use crate::{
    Error, Result,
    acoustics::Acoustics,
    voice::{AudioSettings, Edge, Endpoint, PRE_ROLL_MS},
};
use cpal::{
    Device, FromSample, Sample, SampleFormat,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use serde::Serialize;
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc as sync,
    },
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

fn audio_error(message: impl Into<String>) -> Error {
    Error::Validation(message.into())
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct Devices {
    pub inputs: Vec<AudioDevice>,
    pub outputs: Vec<AudioDevice>,
}
pub fn devices() -> Result<Devices> {
    let host = cpal::default_host();
    let input = host
        .default_input_device()
        .and_then(|d| d.id().ok())
        .map(|i| i.to_string());
    let output = host
        .default_output_device()
        .and_then(|d| d.id().ok())
        .map(|i| i.to_string());
    let list = |items: Vec<Device>, default: Option<String>| {
        items
            .into_iter()
            .filter_map(|d| {
                let id = d.id().ok()?.to_string();
                Some(AudioDevice {
                    is_default: Some(&id) == default.as_ref(),
                    id,
                    name: d.description().ok()?.name().to_string(),
                })
            })
            .collect()
    };
    Ok(Devices {
        inputs: list(
            host.input_devices()
                .map_err(|_| audio_error("Impossible de lister les microphones Windows."))?
                .collect(),
            input,
        ),
        outputs: list(
            host.output_devices()
                .map_err(|_| audio_error("Impossible de lister les sorties audio Windows."))?
                .collect(),
            output,
        ),
    })
}

pub enum AudioEvent {
    Start(Uuid, Vec<i16>),
    Frame(Uuid, Vec<i16>),
    End(Uuid),
    Meter(Uuid, f32, bool),
    Failed(Uuid, String),
}

struct Packet {
    pcm: Vec<i16>,
    position: f64,
    volume: f32,
    token: CancellationToken,
    started: Option<oneshot::Sender<()>>,
    done: Option<oneshot::Sender<()>>,
}

struct Draining {
    at: Instant,
    token: CancellationToken,
    done: Option<oneshot::Sender<()>>,
}

#[derive(Default)]
struct PlaybackQueue {
    packets: VecDeque<Packet>,
    draining: VecDeque<Draining>,
}

impl PlaybackQueue {
    fn acknowledge(&mut self, now: Instant) {
        while self
            .draining
            .front()
            .is_some_and(|d| d.at <= now || d.token.is_cancelled())
        {
            let mut drained = self.draining.pop_front().unwrap();
            if !drained.token.is_cancelled()
                && let Some(done) = drained.done.take()
            {
                let _ = done.send(());
            }
        }
    }

    fn sample(&mut self, rate: u32, now: Instant, latency: Duration) -> f32 {
        while self.packets.front().is_some_and(|p| p.token.is_cancelled()) {
            self.packets.pop_front();
        }
        let Some(packet) = self.packets.front_mut() else {
            return 0.0;
        };
        if let Some(tx) = packet.started.take() {
            let _ = tx.send(());
        }
        let index = packet.position as usize;
        let a = f32::from(packet.pcm[index]) / 32768.0;
        let b = f32::from(*packet.pcm.get(index + 1).unwrap_or(&packet.pcm[index])) / 32768.0;
        let value = (a + (b - a) * packet.position.fract() as f32) * packet.volume;
        packet.position += 24_000.0 / f64::from(rate);
        if packet.position as usize >= packet.pcm.len() {
            let packet = self.packets.pop_front().unwrap();
            self.draining.push_back(Draining {
                at: now + latency + Duration::from_secs_f64(1.0 / f64::from(rate)),
                token: packet.token,
                done: packet.done,
            });
        }
        value
    }
}

#[derive(Clone, Default)]
pub struct Playback(
    Arc<Mutex<PlaybackQueue>>,
    Arc<Mutex<Option<CancellationToken>>>,
);
pub struct Receipt {
    pub started: oneshot::Receiver<()>,
    pub done: oneshot::Receiver<()>,
}

impl Playback {
    pub fn arm(&self, token: CancellationToken) -> Result<()> {
        let mut generation = self
            .1
            .lock()
            .map_err(|_| audio_error("Moteur audio indisponible."))?;
        if token.is_cancelled() {
            return Err(Error::StaleOperation);
        }
        *generation = Some(token);
        Ok(())
    }
    pub fn responding(&self) -> bool {
        self.1
            .lock()
            .is_ok_and(|t| t.as_ref().is_some_and(|t| !t.is_cancelled()))
    }
    pub fn interrupt(&self) {
        // Also cancels a generation waiting on LM Studio/TTS, before it has
        // queued any PCM. Otherwise it could begin speaking after barge-in.
        if let Ok(mut generation) = self.1.lock()
            && let Some(token) = generation.take()
        {
            token.cancel();
        }
        if let Ok(mut slot) = self.0.lock() {
            for packet in &slot.packets {
                packet.token.cancel();
            }
            for packet in &slot.draining {
                packet.token.cancel();
            }
            slot.packets.clear();
            slot.draining.clear();
        }
    }
    pub fn speaking(&self) -> bool {
        self.0.lock().is_ok_and(|slot| {
            slot.packets.iter().any(|p| !p.token.is_cancelled())
                || slot.draining.iter().any(|p| !p.token.is_cancelled())
        })
    }
    pub fn enqueue(&self, pcm: Vec<i16>, volume: f32, token: CancellationToken) -> Result<Receipt> {
        if token.is_cancelled() {
            return Err(Error::StaleOperation);
        }
        if pcm.is_empty() || pcm.len() > 24_000 * 45 {
            return Err(audio_error("Phrase audio vide ou trop longue."));
        }
        let (start_tx, started) = oneshot::channel();
        let (done_tx, done) = oneshot::channel();
        let mut slot = self
            .0
            .lock()
            .map_err(|_| audio_error("La sortie audio doit être redémarrée."))?;
        slot.packets.retain(|p| !p.token.is_cancelled());
        slot.draining.retain(|p| !p.token.is_cancelled());
        if token.is_cancelled() {
            return Err(Error::StaleOperation);
        }
        if slot.packets.len() + slot.draining.len() >= 2 {
            return Err(audio_error("Le tampon de lecture audio est plein."));
        }
        slot.packets.push_back(Packet {
            pcm,
            position: 0.0,
            volume,
            token,
            started: Some(start_tx),
            done: Some(done_tx),
        });
        Ok(Receipt { started, done })
    }
}

struct Run {
    id: Uuid,
    cancel: CancellationToken,
    capture: bool,
    options: AudioSettings,
    thread: std::thread::JoinHandle<()>,
}
pub struct AudioController {
    run: Option<Run>,
    pub playback: Playback,
    events: mpsc::Sender<AudioEvent>,
}
impl AudioController {
    pub fn new(events: mpsc::Sender<AudioEvent>) -> Self {
        Self {
            run: None,
            playback: Playback::default(),
            events,
        }
    }
    pub fn current(&self, id: Uuid) -> bool {
        self.run
            .as_ref()
            .is_some_and(|r| r.id == id && !r.cancel.is_cancelled())
    }
    pub fn owns(&self, id: Uuid) -> bool {
        self.run.as_ref().is_some_and(|r| r.id == id)
    }
    pub fn stop(&mut self) {
        self.playback.interrupt();
        if let Some(run) = self.run.take() {
            run.cancel.cancel();
            let _ = run.thread.join();
        }
    }
    pub fn ensure(&mut self, options: AudioSettings, capture: bool) -> Result<Uuid> {
        if let Some(run) = self.run.as_ref()
            && !run.cancel.is_cancelled()
            && run.options == options
            && (!capture || run.capture)
        {
            return Ok(run.id);
        }
        self.stop();
        options.validate()?;
        let id = Uuid::new_v4();
        let cancel = CancellationToken::new();
        let thread_cancel = cancel.clone();
        let playback = self.playback.clone();
        let events = self.events.clone();
        let thread_options = options.clone();
        let (tx, rx) = sync::sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("sherka-audio".into())
            .spawn(move || {
                let result = run_audio(
                    id,
                    thread_options,
                    capture,
                    playback,
                    &thread_cancel,
                    &events,
                    tx,
                );
                if let Err(error) = result {
                    // Data messages reserve one slot for this terminal event.
                    let _ = events.try_send(AudioEvent::Failed(id, error.to_string()));
                }
                thread_cancel.cancel();
            })?;
        // Initialization occurs on the audio owner thread; callers use a
        // blocking task, never the UI/event loop or the core mutex.
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok(())) => {
                self.run = Some(Run {
                    id,
                    cancel,
                    capture,
                    options,
                    thread,
                });
                Ok(id)
            }
            Ok(Err(message)) => {
                cancel.cancel();
                let _ = thread.join();
                Err(audio_error(message))
            }
            Err(_) => {
                cancel.cancel();
                Err(audio_error("L'ouverture du périphérique audio a expiré."))
            }
        }
    }
}
impl Drop for AudioController {
    fn drop(&mut self) {
        self.stop();
    }
}

struct Frame {
    samples: Vec<f32>,
    capture: bool,
    at: Instant,
}
struct FrameSink {
    samples: Vec<f32>,
    size: usize,
    capture: bool,
    tx: sync::SyncSender<Frame>,
    failed: Arc<AtomicBool>,
}
impl FrameSink {
    fn push(&mut self, sample: f32) {
        self.samples.push(sample);
        if self.samples.len() == self.size {
            let samples = std::mem::replace(&mut self.samples, Vec::with_capacity(self.size));
            if self
                .tx
                .try_send(Frame {
                    samples,
                    capture: self.capture,
                    at: Instant::now(),
                })
                .is_err()
            {
                self.failed.store(true, Ordering::Release);
            }
        }
    }
}

fn choose(host: &cpal::Host, id: &Option<String>, input: bool) -> Result<Device> {
    let chosen = if input {
        host.default_input_device()
    } else {
        host.default_output_device()
    };
    let chosen_id = id
        .clone()
        .or_else(|| chosen.and_then(|d| d.id().ok()).map(|v| v.to_string()));
    // Enumerating gives a fixed endpoint, avoiding an unannounced WASAPI
    // default-device reroute while microphone consent is active.
    let mut devices = host
        .devices()
        .map_err(|_| audio_error("Les périphériques audio Windows sont indisponibles."))?;
    devices
        .find(|d| {
            d.id()
                .ok()
                .is_some_and(|i| Some(i.to_string()) == chosen_id)
        })
        .ok_or_else(|| {
            audio_error(if input {
                "Microphone introuvable. Choisis un périphérique dans les réglages."
            } else {
                "Sortie audio introuvable. Choisis un périphérique dans les réglages."
            })
        })
}

fn input_stream<T>(
    device: &Device,
    config: cpal::StreamConfig,
    mut sink: FrameSink,
    failed: Arc<AtomicBool>,
    latency: Arc<AtomicU64>,
) -> Result<cpal::Stream>
where
    T: cpal::SizedSample,
    f32: FromSample<T>,
{
    let channels = usize::from(config.channels);
    device.build_input_stream(config,move |data: &[T], info: &cpal::InputCallbackInfo| {
        let time=info.timestamp(); latency.store(time.callback.duration_since(time.capture).as_millis() as u64,Ordering::Relaxed);
        for frame in data.chunks_exact(channels) { sink.push(frame.iter().map(|s| f32::from_sample(*s)).sum::<f32>()/channels as f32); }
    },move |_| {failed.store(true,Ordering::Release);},Some(Duration::from_secs(3)))
    .map_err(|_| audio_error("Windows refuse le microphone. Vérifie les autorisations et le périphérique choisi."))
}

fn output_stream<T>(
    device: &Device,
    config: cpal::StreamConfig,
    mut sink: FrameSink,
    playback: Playback,
    failed: Arc<AtomicBool>,
    latency: Arc<AtomicU64>,
) -> Result<cpal::Stream>
where
    T: cpal::SizedSample + FromSample<f32>,
{
    let channels = usize::from(config.channels);
    let rate = config.sample_rate;
    device
        .build_output_stream(
            config,
            move |data: &mut [T], info: &cpal::OutputCallbackInfo| {
                let time = info.timestamp();
                let delay = time.playback.duration_since(time.callback);
                latency.store(delay.as_millis() as u64, Ordering::Relaxed);
                // The render reference is exactly the locally scaled PCM written to
                // the hardware buffer, including zeros and cancellation boundaries.
                if let Ok(mut slot) = playback.0.try_lock() {
                    let now = Instant::now();
                    slot.acknowledge(now);
                    for (offset, frame) in data.chunks_exact_mut(channels).enumerate() {
                        let sample_delay =
                            delay + Duration::from_secs_f64(offset as f64 / f64::from(rate));
                        let sample = slot.sample(rate, now, sample_delay);
                        frame.fill(T::from_sample(sample));
                        sink.push(sample);
                    }
                } else {
                    for frame in data.chunks_exact_mut(channels) {
                        frame.fill(T::from_sample(0.0));
                        sink.push(0.0);
                    }
                }
            },
            move |_| {
                failed.store(true, Ordering::Release);
            },
            Some(Duration::from_secs(3)),
        )
        .map_err(|_| audio_error("Windows refuse la sortie audio choisie."))
}

fn run_audio(
    id: Uuid,
    options: AudioSettings,
    capture: bool,
    playback: Playback,
    cancel: &CancellationToken,
    events: &mpsc::Sender<AudioEvent>,
    ready: sync::SyncSender<std::result::Result<(), String>>,
) -> Result<()> {
    let host = cpal::default_host();
    let (frames_tx, frames_rx) = sync::sync_channel(100);
    let failed = Arc::new(AtomicBool::new(false));
    let in_delay = Arc::new(AtomicU64::new(0));
    let out_delay = Arc::new(AtomicU64::new(0));
    let prepare = || -> Result<_> {
        let output = choose(&host, &options.output_device, false)?;
        let output_id = output
            .id()
            .map_err(|_| audio_error("Sortie audio débranchée."))?
            .to_string();
        let cfg = output
            .default_output_config()
            .map_err(|_| audio_error("Format de sortie audio indisponible."))?;
        let output_rate = cfg.sample_rate();
        let sink = FrameSink {
            samples: Vec::with_capacity(output_rate as usize / 100),
            size: output_rate as usize / 100,
            capture: false,
            tx: frames_tx.clone(),
            failed: failed.clone(),
        };
        let stream = match cfg.sample_format() {
            SampleFormat::F32 => output_stream::<f32>(
                &output,
                cfg.into(),
                sink,
                playback.clone(),
                failed.clone(),
                out_delay.clone(),
            ),
            SampleFormat::I16 => output_stream::<i16>(
                &output,
                cfg.into(),
                sink,
                playback.clone(),
                failed.clone(),
                out_delay.clone(),
            ),
            SampleFormat::I32 => output_stream::<i32>(
                &output,
                cfg.into(),
                sink,
                playback.clone(),
                failed.clone(),
                out_delay.clone(),
            ),
            _ => Err(audio_error(
                "Choisis un format Windows PCM 16/32 bits ou flottant.",
            )),
        }?;
        let mut input_stream_value = None;
        let mut input_rate = 48000;
        let mut input_id = None;
        if capture {
            let input = choose(&host, &options.input_device, true)?;
            input_id = Some(
                input
                    .id()
                    .map_err(|_| audio_error("Microphone débranché."))?
                    .to_string(),
            );
            let cfg = input
                .default_input_config()
                .map_err(|_| audio_error("Format du microphone indisponible."))?;
            input_rate = cfg.sample_rate();
            let sink = FrameSink {
                samples: Vec::with_capacity(input_rate as usize / 100),
                size: input_rate as usize / 100,
                capture: true,
                tx: frames_tx.clone(),
                failed: failed.clone(),
            };
            input_stream_value = Some(match cfg.sample_format() {
                SampleFormat::F32 => {
                    input_stream::<f32>(&input, cfg.into(), sink, failed.clone(), in_delay.clone())
                }
                SampleFormat::I16 => {
                    input_stream::<i16>(&input, cfg.into(), sink, failed.clone(), in_delay.clone())
                }
                SampleFormat::I32 => {
                    input_stream::<i32>(&input, cfg.into(), sink, failed.clone(), in_delay.clone())
                }
                _ => Err(audio_error(
                    "Choisis un microphone PCM 16/32 bits ou flottant.",
                )),
            }?);
        }
        if !(8000..=96000).contains(&input_rate)
            || input_rate % 100 != 0
            || !(8000..=96000).contains(&output_rate)
            || output_rate % 100 != 0
        {
            return Err(audio_error(
                "Choisis une fréquence Windows entre 8 et 96 kHz (44,1 ou 48 kHz conseillés).",
            ));
        }
        let acoustics = Acoustics::new(input_rate, output_rate, options.speakers)?;
        stream
            .play()
            .map_err(|_| audio_error("Impossible de démarrer la sortie audio."))?;
        if let Some(input) = &input_stream_value {
            input
                .play()
                .map_err(|_| audio_error("Impossible de démarrer le microphone."))?;
        }
        Ok((stream, input_stream_value, acoustics, input_id, output_id))
    };
    let (_output, _input, mut acoustics, input_id, output_id) = match prepare() {
        Ok(value) => {
            let _ = ready.send(Ok(()));
            value
        }
        Err(error) => {
            let _ = ready.send(Err(error.to_string()));
            return Ok(());
        }
    };
    // Preserve initial syllables during neural voice/echo confirmation. This
    // history is continuously rolling; it introduces no additional wait.
    let pre_roll_frames = PRE_ROLL_MS / 10;
    let mut endpoint = Endpoint::new(options.clone());
    let mut preroll: VecDeque<Vec<i16>> = VecDeque::with_capacity(pre_roll_frames);
    let mut meter_count = 0;
    let mut last_device_check = Instant::now();
    while !cancel.is_cancelled() {
        if failed.load(Ordering::Acquire) {
            playback.interrupt();
            return Err(audio_error(
                "Flux audio interrompu ou saturé. Vérifie les périphériques puis réactive le micro.",
            ));
        }
        if last_device_check.elapsed() > Duration::from_secs(1) {
            last_device_check = Instant::now();
            let default_in = host
                .default_input_device()
                .and_then(|d| d.id().ok())
                .map(|v| v.to_string());
            let default_out = host
                .default_output_device()
                .and_then(|d| d.id().ok())
                .map(|v| v.to_string());
            if (capture && options.input_device.is_none() && default_in != input_id)
                || (options.output_device.is_none() && default_out.as_ref() != Some(&output_id))
            {
                playback.interrupt();
                return Err(audio_error(
                    "Le périphérique Windows par défaut a changé. Réactive le micro pour utiliser le nouveau périphérique.",
                ));
            }
        }
        let frame = match frames_rx.recv_timeout(Duration::from_millis(25)) {
            Ok(f) => f,
            Err(sync::RecvTimeoutError::Timeout) => continue,
            Err(_) => break,
        };
        if !frame.capture {
            if capture {
                acoustics.render(&frame.samples)?;
            }
            continue;
        }
        let delay = in_delay.load(Ordering::Relaxed)
            + out_delay.load(Ordering::Relaxed)
            + frame.at.elapsed().as_millis() as u64;
        let processed = acoustics.capture(&frame.samples, delay.min(500) as u32)?;
        let pcm = processed.pcm;
        let speech = processed.speech;
        let edge = endpoint.feed(speech, playback.responding());
        meter_count += 1;
        if meter_count == 10 {
            meter_count = 0;
            if events.capacity() > 2 {
                let _ = events.try_send(AudioEvent::Meter(
                    id,
                    (processed.level * 5.0).min(1.0),
                    speech,
                ));
            }
        }
        let event = match edge {
            Edge::Start => {
                // Local silence now: do not wait for STT, HTTP, the event loop,
                // SQLite, or even the next desktop coordinator tick.
                playback.interrupt();
                let mut start: Vec<i16> = preroll.drain(..).flatten().collect();
                start.extend(pcm);
                Some(AudioEvent::Start(id, start))
            }
            Edge::End => Some(AudioEvent::End(id)),
            Edge::None if endpoint.active() => Some(AudioEvent::Frame(id, pcm)),
            _ => {
                if preroll.len() == pre_roll_frames {
                    preroll.pop_front();
                }
                preroll.push_back(pcm);
                None
            }
        };
        if let Some(event) = event
            && (events.capacity() <= 1 || events.try_send(event).is_err())
        {
            playback.interrupt();
            return Err(audio_error(
                "Le traitement vocal n'arrive plus à suivre le microphone. Réactive-le pour reprendre.",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prepared_packets_are_contiguous_but_heard_receipts_wait_for_hardware() {
        let playback = Playback::default();
        let token = CancellationToken::new();
        let mut first = playback
            .enqueue(vec![16384; 240], 1.0, token.clone())
            .unwrap();
        let mut second = playback
            .enqueue(vec![8192; 240], 1.0, token.clone())
            .unwrap();
        let now = Instant::now();
        {
            let mut queue = playback.0.lock().unwrap();
            for i in 0..480 {
                let sample = queue.sample(
                    24000,
                    now,
                    Duration::from_millis(50) + Duration::from_secs_f64(i as f64 / 24000.0),
                );
                assert_eq!(sample, if i < 240 { 0.5 } else { 0.25 });
            }
            queue.acknowledge(now + Duration::from_millis(59));
            assert!(first.done.try_recv().is_err());
            queue.acknowledge(now + Duration::from_millis(60));
            assert!(first.done.try_recv().is_ok());
            assert!(second.done.try_recv().is_err());
        }
        playback.interrupt();
        assert!(second.done.try_recv().is_err());
        let fresh = CancellationToken::new();
        let _new = playback.enqueue(vec![4096; 10], 1.0, fresh).unwrap();
        assert_eq!(
            playback
                .0
                .lock()
                .unwrap()
                .sample(24000, now, Duration::ZERO),
            0.125
        );
    }
    #[test]
    fn canceled_audio_cannot_resume_and_queue_is_bounded() {
        let playback = Playback::default();
        let token = CancellationToken::new();
        let _r = playback
            .enqueue(vec![32000; 2400], 0.8, token.clone())
            .unwrap();
        let _next = playback.enqueue(vec![1; 10], 1.0, token.clone()).unwrap();
        assert!(playback.enqueue(vec![1; 10], 1.0, token.clone()).is_err());
        playback.interrupt();
        assert!(token.is_cancelled());
        assert!(!playback.speaking());
        assert!(playback.enqueue(vec![1; 10], 1.0, token).is_err());
        let waiting = CancellationToken::new();
        playback.arm(waiting.clone()).unwrap();
        assert!(!playback.speaking());
        assert!(playback.responding());
        playback.interrupt();
        assert!(waiting.is_cancelled());
        assert!(playback.arm(waiting).is_err());
    }
    #[test]
    fn apm_accepts_windows_rates_and_silent_audio_never_triggers_vad() {
        for rate in [16000, 44100, 48000, 96000] {
            let mut acoustics = Acoustics::new(rate, rate, true).unwrap();
            let frame = vec![0.0; rate as usize / 100];
            acoustics.render(&frame).unwrap();
            let captured = acoustics.capture(&frame, 20).unwrap();
            assert_eq!(captured.pcm.len(), 160);
            assert!(!captured.speech);
        }
    }
}
