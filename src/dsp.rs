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
            out[c] = if y.is_finite() { y } else { 0. };
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
