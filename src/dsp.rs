#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    z: [[f32; 2]; 2],
}
impl Default for Biquad {
    fn default() -> Self {
        Self {
            b: [1., 0., 0.],
            a: [0.; 2],
            z: [[0.; 2]; 2],
        }
    }
}
impl Biquad {
    pub fn configure(&mut self, band: usize, gain: f32, sample_rate: u32) {
        let a = 10.0f32.powf(gain / 40.);
        let w = std::f32::consts::TAU * [4000., 1000., 250.][band] / sample_rate as f32;
        let (s, c) = w.sin_cos();
        let alpha = s / 1.4;
        let beta = 2. * a.sqrt() * alpha;
        let (b, aa) = match band {
            0 => (
                [
                    a * ((a + 1.) + (a - 1.) * c + beta),
                    -2. * a * ((a - 1.) + (a + 1.) * c),
                    a * ((a + 1.) + (a - 1.) * c - beta),
                ],
                [
                    (a + 1.) - (a - 1.) * c + beta,
                    2. * ((a - 1.) - (a + 1.) * c),
                    (a + 1.) - (a - 1.) * c - beta,
                ],
            ),
            1 => (
                [1. + alpha * a, -2. * c, 1. - alpha * a],
                [1. + alpha / a, -2. * c, 1. - alpha / a],
            ),
            _ => (
                [
                    a * ((a + 1.) - (a - 1.) * c + beta),
                    2. * a * ((a - 1.) - (a + 1.) * c),
                    a * ((a + 1.) - (a - 1.) * c - beta),
                ],
                [
                    (a + 1.) + (a - 1.) * c + beta,
                    -2. * ((a - 1.) + (a + 1.) * c),
                    (a + 1.) + (a - 1.) * c - beta,
                ],
            ),
        };
        self.b = b.map(|x| x / aa[0]);
        self.a = [aa[1] / aa[0], aa[2] / aa[0]];
    }
    pub fn process(&mut self, frame: [f32; 2]) -> [f32; 2] {
        let mut out = [0.; 2];
        for c in 0..2 {
            let y = self.b[0] * frame[c] + self.z[c][0];
            self.z[c][0] = self.b[1] * frame[c] - self.a[0] * y + self.z[c][1];
            self.z[c][1] = self.b[2] * frame[c] - self.a[1] * y;
            if y.is_finite() {
                out[c] = y;
            } else {
                // A non-finite value would otherwise stay in the state forever and mute the deck.
                self.z[c] = [0.; 2];
                out[c] = 0.;
            }
        }
        out
    }
}

pub fn cross_gains(value: f32, linear: bool) -> [f32; 2] {
    let x = value.clamp(0., 1.);
    if linear {
        [1. - x, x]
    } else {
        let (s, c) = (x * std::f32::consts::FRAC_PI_2).sin_cos();
        [c, s]
    }
}

/// Real-time delay-line pitch shifter used for key lock.
/// Two read taps sweep through a short window with complementary sin² gains,
/// so a deck played at speed `s` is corrected back to its original key with ratio `1/s`.
#[derive(Clone)]
pub struct PitchShifter {
    buffer: Vec<[f32; 2]>,
    at: usize,
    phase: f64,
    window: f64,
}
impl PitchShifter {
    pub fn new(sample_rate: u32) -> Self {
        let window = (sample_rate.max(8000) as f64 * 0.045).round();
        Self {
            buffer: vec![[0.; 2]; window as usize + 8],
            at: 0,
            phase: 0.,
            window,
        }
    }
    pub fn reset(&mut self) {
        self.buffer.fill([0.; 2]);
        self.at = 0;
        self.phase = 0.;
    }
    fn tap(&self, delay: f64) -> [f32; 2] {
        let len = self.buffer.len() as f64;
        let pos = (self.at as f64 - delay).rem_euclid(len);
        let a = pos as usize % self.buffer.len();
        let b = (a + 1) % self.buffer.len();
        let t = (pos - pos.floor()) as f32;
        std::array::from_fn(|c| self.buffer[a][c] * (1. - t) + self.buffer[b][c] * t)
    }
    /// Writes `input` and returns it shifted by `ratio` (1.0 = unchanged).
    pub fn process(&mut self, input: [f32; 2], ratio: f64) -> [f32; 2] {
        self.buffer[self.at] = input;
        self.at = (self.at + 1) % self.buffer.len();
        if !ratio.is_finite() || (ratio - 1.).abs() < 1e-4 {
            return input;
        }
        self.phase = (self.phase + (1. - ratio.clamp(0.25, 4.)) / self.window).rem_euclid(1.);
        let second = (self.phase + 0.5).rem_euclid(1.);
        let a = self.tap(2. + self.phase * self.window);
        let b = self.tap(2. + second * self.window);
        let ga = (std::f64::consts::PI * self.phase).sin().powi(2) as f32;
        let gb = 1. - ga;
        std::array::from_fn(|c| a[c] * ga + b[c] * gb)
    }
}
