//! `record.tap`: copies the interleaved stereo frames the transcriber receives into a WAV,
//! for the accuracy evaluation set (`tools/accuracy/`). The audio is personal; the file is 0600.

use std::io::BufWriter;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

pub struct Tap {
    writer: hound::WavWriter<BufWriter<std::fs::File>>,
    /// Samples (both channels) still to write.
    left: u64,
}

impl Tap {
    pub fn create(path: &Path, seconds: u64) -> Result<Self, String> {
        if !path.is_absolute() || path.extension().is_none_or(|e| e != "wav") {
            return Err("record.tap needs an absolute path ending in .wav".into());
        }
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let writer =
            hound::WavWriter::new(BufWriter::new(file), spec).map_err(|e| e.to_string())?;
        Ok(Self {
            writer,
            left: seconds.saturating_mul(32_000),
        })
    }

    /// Append interleaved samples; false when the tap is complete or failed (then call `finish`).
    pub fn write(&mut self, samples: &[f32]) -> bool {
        let take = samples.len().min(self.left as usize) & !1;
        for s in &samples[..take] {
            if self
                .writer
                .write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16)
                .is_err()
            {
                return false;
            }
        }
        self.left -= take as u64;
        self.left > 0
    }

    pub fn finish(self) {
        let _ = self.writer.finalize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn writes_stereo_16k_frames_up_to_the_limit_with_mode_0600() {
        let path = std::env::temp_dir().join(format!("tap-{}/sub/t.wav", std::process::id()));
        let mut tap = Tap::create(&path, 1).unwrap();
        assert!(tap.write(&[0.5, -0.5, 2.0, -2.0]));
        assert!(!tap.write(&vec![0.0; 40_000]));
        tap.finish();
        assert!(Tap::create(&path, 1).is_err(), "never overwrites");
        assert!(Tap::create(Path::new("rel.wav"), 1).is_err());
        assert!(Tap::create(&path.with_extension("txt"), 1).is_err());
        assert_eq!(path.metadata().unwrap().permissions().mode() & 0o777, 0o600);
        let mut r = hound::WavReader::open(&path).unwrap();
        assert_eq!((r.spec().channels, r.spec().sample_rate), (2, 16_000));
        assert_eq!(r.duration(), 16_000);
        let first: Vec<i16> = r.samples::<i16>().take(4).map(Result::unwrap).collect();
        assert_eq!(first, [16383, -16383, 32767, -32767]);
        std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).unwrap();
    }
}
