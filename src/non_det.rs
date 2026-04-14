use std::{
    fmt::{self, Debug},
    hash::Hash,
    sync::{Arc, Mutex, MutexGuard},
};

#[cfg(test)]
use std::{collections::hash_map::DefaultHasher, hash::Hasher};

use argon2::password_hash::SaltString;
#[cfg(test)]
use chrono::{Duration, NaiveDate};
use chrono::{NaiveDateTime, Utc};
use rand::{Rng, RngCore, SeedableRng, distr::Alphanumeric};
use rand_chacha::ChaCha20Rng;
use serde::{Serialize, de::DeserializeOwned};
use uuid::{Builder, Uuid};

#[derive(Clone)]
pub struct NonDet {
    inner: Arc<Mutex<NonDetInner>>,
}

enum NonDetInner {
    Production,
    #[cfg(test)]
    Deterministic(DeterministicNonDet),
}

#[cfg(test)]
struct DeterministicNonDet {
    rng: ChaCha20Rng,
    tick: u64,
    capture: CaptureMode,
}

#[cfg(test)]
#[derive(Clone)]
pub struct NonDetSnapshot {
    rng: ChaCha20Rng,
    tick: u64,
    capture: CaptureMode,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct CapturedInput {
    pub tick: u64,
    pub input_hash: u64,
    pub type_name: String,
    pub output: serde_json::Value,
}

#[cfg(test)]
#[derive(Clone)]
enum CaptureMode {
    Off,
    Record(Vec<CapturedInput>),
    Replay {
        entries: Vec<CapturedInput>,
        pos: usize,
    },
}

pub struct NonDetRng<'a> {
    inner: MutexGuard<'a, NonDetInner>,
}

impl RngCore for NonDetRng<'_> {
    fn next_u32(&mut self) -> u32 {
        match &mut *self.inner {
            NonDetInner::Production => rand::rng().next_u32(),
            #[cfg(test)]
            NonDetInner::Deterministic(det) => det.rng.next_u32(),
        }
    }

    fn next_u64(&mut self) -> u64 {
        match &mut *self.inner {
            NonDetInner::Production => rand::rng().next_u64(),
            #[cfg(test)]
            NonDetInner::Deterministic(det) => det.rng.next_u64(),
        }
    }

    fn fill_bytes(&mut self, dst: &mut [u8]) {
        match &mut *self.inner {
            NonDetInner::Production => rand::rng().fill_bytes(dst),
            #[cfg(test)]
            NonDetInner::Deterministic(det) => det.rng.fill_bytes(dst),
        }
    }
}

impl Default for NonDet {
    fn default() -> Self {
        Self::production()
    }
}

impl Debug for NonDet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NonDet").finish_non_exhaustive()
    }
}

impl NonDet {
    pub fn production() -> Self {
        Self {
            inner: Arc::new(Mutex::new(NonDetInner::Production)),
        }
    }

    #[cfg(test)]
    pub fn deterministic(seed: u64) -> Self {
        Self::deterministic_with_capture(seed, CaptureMode::Off)
    }

    #[cfg(test)]
    pub fn deterministic_recording(seed: u64) -> Self {
        Self::deterministic_with_capture(seed, CaptureMode::Record(Vec::new()))
    }

    #[cfg(test)]
    pub fn deterministic_replay(
        seed: u64,
        entries: Vec<CapturedInput>,
    ) -> Self {
        Self::deterministic_with_capture(
            seed,
            CaptureMode::Replay { entries, pos: 0 },
        )
    }

    #[cfg(test)]
    fn deterministic_with_capture(seed: u64, capture: CaptureMode) -> Self {
        Self {
            inner: Arc::new(Mutex::new(NonDetInner::Deterministic(
                DeterministicNonDet {
                    rng: ChaCha20Rng::seed_from_u64(seed),
                    tick: 0,
                    capture,
                },
            ))),
        }
    }

    #[cfg(test)]
    pub fn from_snapshot(snapshot: NonDetSnapshot) -> Self {
        Self {
            inner: Arc::new(Mutex::new(NonDetInner::Deterministic(
                DeterministicNonDet {
                    rng: snapshot.rng,
                    tick: snapshot.tick,
                    capture: snapshot.capture,
                },
            ))),
        }
    }

    pub fn rng(&self) -> NonDetRng<'_> {
        NonDetRng {
            inner: self.inner.lock().unwrap(),
        }
    }

    pub fn uuid_now_v7(&self) -> Uuid {
        let millis = self.now_utc_naive().and_utc().timestamp_millis();
        let millis = u64::try_from(millis).unwrap_or(0);
        let counter_random_bytes: [u8; 10] = self.rng().random();
        Builder::from_unix_timestamp_millis(millis, &counter_random_bytes)
            .into_uuid()
    }

    pub fn uuid_v4(&self) -> Uuid {
        let random_bytes: [u8; 16] = self.rng().random();
        Builder::from_random_bytes(random_bytes).into_uuid()
    }

    pub fn now_utc_naive(&self) -> NaiveDateTime {
        self.with_inner(|inner| match inner {
            NonDetInner::Production => Utc::now().naive_utc(),
            #[cfg(test)]
            NonDetInner::Deterministic(det) => {
                let base = NaiveDate::from_ymd_opt(2024, 1, 1)
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap();
                let now = base + Duration::milliseconds(det.tick as i64);
                det.tick += 1;
                now
            }
        })
    }

    pub fn fork_rng(&self) -> ChaCha20Rng {
        ChaCha20Rng::seed_from_u64(self.rng().random())
    }

    pub fn alphanumeric_string(&self, len: usize) -> String {
        let mut rng = self.rng();
        (&mut rng)
            .sample_iter(&Alphanumeric)
            .take(len)
            .map(char::from)
            .collect()
    }

    pub fn password_salt(&self) -> SaltString {
        let bytes: [u8; 16] = self.rng().random();
        SaltString::encode_b64(&bytes).unwrap()
    }

    pub fn wrap<I, T, F>(&self, _inputs: &I, f: F) -> T
    where
        I: Hash + ?Sized,
        T: Clone + Debug + PartialEq + Serialize + DeserializeOwned,
        F: FnOnce() -> T,
    {
        self.with_inner(|inner| match inner {
            NonDetInner::Production => f(),
            #[cfg(test)]
            NonDetInner::Deterministic(det) => {
                let input_hash = hash_input(_inputs);
                let tick = det.tick;
                det.tick += 1;
                match &mut det.capture {
                    CaptureMode::Off => f(),
                    CaptureMode::Record(entries) => {
                        let output = f();
                        entries.push(CapturedInput {
                            tick,
                            input_hash,
                            type_name: std::any::type_name::<T>().to_string(),
                            output: serde_json::to_value(&output).unwrap(),
                        });
                        output
                    }
                    CaptureMode::Replay { entries, pos } => {
                        let entry = entries.get(*pos).unwrap_or_else(|| {
                            panic!("missing captured nondeterministic input at tick {tick}")
                        });
                        assert_eq!(entry.tick, tick);
                        assert_eq!(entry.input_hash, input_hash);
                        assert_eq!(entry.type_name, std::any::type_name::<T>());
                        *pos += 1;
                        serde_json::from_value(entry.output.clone()).unwrap()
                    }
                }
            }
        })
    }

    #[cfg(test)]
    pub fn captured_inputs(&self) -> Vec<CapturedInput> {
        self.with_inner(|inner| match inner {
            NonDetInner::Production => Vec::new(),
            NonDetInner::Deterministic(det) => match &det.capture {
                CaptureMode::Record(entries) => entries.clone(),
                CaptureMode::Replay { entries, .. } => entries.clone(),
                CaptureMode::Off => Vec::new(),
            },
        })
    }

    #[cfg(test)]
    pub fn snapshot(&self) -> NonDetSnapshot {
        self.with_inner(|inner| match inner {
            NonDetInner::Production => {
                panic!("production NonDet cannot be snapshotted")
            }
            NonDetInner::Deterministic(det) => NonDetSnapshot {
                rng: det.rng.clone(),
                tick: det.tick,
                capture: det.capture.clone(),
            },
        })
    }

    #[cfg(test)]
    pub fn assert_replay_finished(&self) {
        self.with_inner(|inner| {
            if let NonDetInner::Deterministic(DeterministicNonDet {
                capture: CaptureMode::Replay { entries, pos },
                ..
            }) = inner
            {
                assert_eq!(*pos, entries.len());
            }
        });
    }

    pub fn next_probe_u64(&self) -> u64 {
        self.rng().random()
    }

    fn with_inner<T>(&self, f: impl FnOnce(&mut NonDetInner) -> T) -> T {
        let mut guard = self.inner.lock().unwrap();
        f(&mut guard)
    }
}

#[cfg(test)]
fn hash_input<I: Hash + ?Sized>(input: &I) -> u64 {
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::NonDet;

    #[test]
    fn wrap_records_and_replays_outputs_by_tick_and_input() {
        let recorder = NonDet::deterministic_recording(9);
        let value = recorder.wrap(&("solver", 1), || "captured".to_string());
        assert_eq!(value, "captured");
        let captured = recorder.captured_inputs();

        let replay = NonDet::deterministic_replay(9, captured);
        let replayed = replay.wrap(&("solver", 1), || "different".to_string());
        assert_eq!(replayed, "captured");
        replay.assert_replay_finished();
    }

    #[test]
    fn deterministic_rng_reaches_same_probe_after_same_calls() {
        let first = NonDet::deterministic(42);
        let second = NonDet::deterministic(42);

        assert_eq!(first.uuid_now_v7(), second.uuid_now_v7());
        assert_eq!(
            first.alphanumeric_string(12),
            second.alphanumeric_string(12)
        );
        assert_eq!(first.next_probe_u64(), second.next_probe_u64());
    }
}
