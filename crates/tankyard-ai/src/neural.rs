//! Small online motion predictor. Training uses successive visible positions,
//! never the opponent's controls or hidden velocity. No external model/runtime.
const INPUTS: usize = 6;
const HIDDEN: usize = 8;

pub struct MotionNet {
    hidden: [[f32; INPUTS]; HIDDEN],
    output: [[f32; HIDDEN + 1]; 2],
    pub samples: u32,
}
impl Default for MotionNet {
    fn default() -> Self {
        let mut seed = 0x7459u32;
        let hidden = std::array::from_fn(|_| {
            std::array::from_fn(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                (seed as f32 / u32::MAX as f32 - 0.5) * 0.5
            })
        });
        Self {
            hidden,
            output: [[0.0; HIDDEN + 1]; 2],
            samples: 0,
        }
    }
}
impl MotionNet {
    fn forward(&self, input: [f32; INPUTS]) -> ([f32; HIDDEN], [f32; 2]) {
        let hidden = self.hidden.map(|weights| {
            weights
                .into_iter()
                .zip(input)
                .map(|(w, x)| w * x)
                .sum::<f32>()
                .tanh()
        });
        let output = self.output.map(|weights| {
            (weights[..HIDDEN]
                .iter()
                .zip(hidden)
                .map(|(w, h)| w * h)
                .sum::<f32>()
                + weights[HIDDEN])
                .tanh()
        });
        (hidden, output)
    }
    pub fn predict(&self, input: [f32; INPUTS]) -> [f32; 2] {
        self.forward(input).1
    }
    pub fn train(&mut self, input: [f32; INPUTS], target: [f32; 2]) {
        if input.iter().chain(target.iter()).any(|v| !v.is_finite()) {
            return;
        }
        let input = input.map(|v| v.clamp(-1.0, 1.0));
        let (hidden, output) = self.forward(input);
        let delta: [f32; 2] = std::array::from_fn(|i| {
            (target[i].clamp(-1.0, 1.0) - output[i]).clamp(-0.5, 0.5)
                * (1.0 - output[i] * output[i])
        });
        let hidden_delta: [f32; HIDDEN] = std::array::from_fn(|j| {
            (delta[0] * self.output[0][j] + delta[1] * self.output[1][j])
                * (1.0 - hidden[j] * hidden[j])
        });
        let update = |weight: &mut f32, gradient: f32| {
            *weight = (*weight * 0.99999 + 0.035 * gradient).clamp(-2.0, 2.0);
        };
        for (i, weights) in self.output.iter_mut().enumerate() {
            for j in 0..HIDDEN {
                update(&mut weights[j], delta[i] * hidden[j]);
            }
            update(&mut weights[HIDDEN], delta[i]);
        }
        for (weights, delta) in self.hidden.iter_mut().zip(hidden_delta) {
            for (weight, value) in weights.iter_mut().zip(input) {
                update(weight, delta * value);
            }
        }
        self.samples = self.samples.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn learns_motion_trends_and_generalizes_to_unseen_observations() {
        let mut net = MotionNet::default();
        let held_out = [0.25, -0.2, 0.35, -0.45, 0.5, 1.0];
        let target = [held_out[2] * 0.6, held_out[3] * 0.6];
        let error = |net: &MotionNet| {
            net.predict(held_out)
                .into_iter()
                .zip(target)
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f32>()
        };
        let before = error(&net);
        for i in 0..4000 {
            let x = ((i * 17 % 101) as f32 / 50.0 - 1.0) * 0.7;
            let y = ((i * 31 % 103) as f32 / 51.0 - 1.0) * 0.7;
            net.train([0.25, -0.2, x, y, 0.5, 1.0], [x * 0.6, y * 0.6]);
        }
        assert!(
            error(&net) < before * 0.2,
            "prediction must improve on an unseen sample"
        );
        assert_eq!(net.samples, 4000);
    }
    #[test]
    fn noisy_or_invalid_training_cannot_create_unbounded_outputs_or_weights() {
        let mut net = MotionNet::default();
        net.train([f32::NAN; INPUTS], [f32::INFINITY; 2]);
        assert_eq!(net.samples, 0);
        for i in 0..20_000 {
            net.train(
                [if i % 2 == 0 { 100.0 } else { -100.0 }; INPUTS],
                [100.0, -100.0],
            );
        }
        assert!(
            net.hidden
                .iter()
                .flatten()
                .chain(net.output.iter().flatten())
                .all(|w| w.is_finite() && w.abs() <= 2.0)
        );
        assert!(
            net.predict([1.0; INPUTS])
                .iter()
                .all(|p| p.is_finite() && p.abs() <= 1.0)
        );
    }
}
