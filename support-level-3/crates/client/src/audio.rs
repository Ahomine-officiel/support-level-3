//! Audio (rodio) : ambiance, battements de cœur, SFX. Feature "audio".

#[cfg(feature = "audio")]
pub mod backend {
    use rodio::source::Source;
    use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};
    use std::collections::HashMap;
    use std::io::BufReader;
    use std::path::Path;

    pub struct Audio {
        _stream: OutputStream,
        handle: OutputStreamHandle,
        loops: HashMap<String, Sink>,
        volume: f32,
    }

    impl Audio {
        pub fn new(volume: f32) -> Option<Audio> {
            match OutputStream::try_default() {
                Ok((stream, handle)) => Some(Audio {
                    _stream: stream,
                    handle,
                    loops: HashMap::new(),
                    volume,
                }),
                Err(_) => None, // pas de périphérique audio : jeu muet
            }
        }

        pub fn set_volume(&mut self, v: f32) {
            self.volume = v;
            for s in self.loops.values() {
                s.set_volume(v);
            }
        }

        fn load_sink(&self, name: &str, looping: bool) -> Option<Sink> {
            let path = Path::new("assets/audio").join(format!("{name}.wav"));
            let file = std::fs::File::open(path).ok()?;
            let src = Decoder::new(BufReader::new(file)).ok()?;
            let sink = Sink::try_new(&self.handle).ok()?;
            if looping {
                sink.append(src.repeat_infinite());
            } else {
                sink.append(src);
            }
            Some(sink)
        }

        /// Démarre une boucle (ambiance, murmures…).
        pub fn start_loop(&mut self, name: &str, gain: f32) {
            if self.loops.contains_key(name) {
                return;
            }
            if let Some(sink) = self.load_sink(name, true) {
                sink.set_volume(self.volume * gain);
                self.loops.insert(name.to_string(), sink);
            }
        }

        pub fn set_loop_volume(&self, name: &str, gain: f32) {
            if let Some(s) = self.loops.get(name) {
                s.set_volume(self.volume * gain.clamp(0.0, 1.0));
            }
        }

        pub fn stop_loop(&mut self, name: &str) {
            if let Some(s) = self.loops.remove(name) {
                s.stop();
            }
        }

        /// Son ponctuel avec gain 0..1 (atténuation par distance côté appelant).
        pub fn play(&self, name: &str, gain: f32) {
            if gain <= 0.01 {
                return;
            }
            if let Some(sink) = self.load_sink(name, false) {
                sink.set_volume((self.volume * gain).clamp(0.0, 1.0));
                sink.detach();
            }
        }
    }
}

#[cfg(feature = "audio")]
pub use backend::Audio;

#[cfg(not(feature = "audio"))]
pub struct DummyAudio;
#[cfg(not(feature = "audio"))]
impl DummyAudio {
    pub fn new(_: f32) -> Option<Self> {
        Some(DummyAudio)
    }
    pub fn start_loop(&mut self, _: &str, _: f32) {}
    pub fn set_loop_volume(&self, _: &str, _: f32) {}
    pub fn stop_loop(&mut self, _: &str) {}
    pub fn play(&self, _: &str, _: f32) {}
    pub fn set_volume(&mut self, _: f32) {}
}
#[cfg(not(feature = "audio"))]
pub use DummyAudio as Audio;
