//! Port of `creature/mood.py`: emotional state and expressive feeler
//! (antenna) geometry. Free of any creature dependency.

use serde::{Deserialize, Serialize};

#[inline]
fn clamp(value: f64, low: f64, high: f64) -> f64 {
    low.max(high.min(value))
}

/// `approach(current, target, dt, rate)`: frame-rate independent easing.
pub fn approach(current: f64, target: f64, dt: f64, rate: f64) -> f64 {
    if rate <= 0.0 {
        return target;
    }
    let alpha = 1.0 - (-rate * dt).exp();
    current + (target - current) * alpha
}

/// `MOOD_BASELINES`: (valence, arousal, affection, curiosity).
pub const MOOD_BASELINES: [(&str, [f64; 4]); 15] = [
    ("auto", [0.18, 0.40, 0.32, 0.52]),
    ("playful", [0.62, 0.74, 0.40, 0.66]),
    ("cuddly", [0.55, 0.34, 0.85, 0.45]),
    ("curious", [0.28, 0.50, 0.30, 0.92]),
    ("calm", [0.22, 0.18, 0.34, 0.30]),
    ("skittish", [-0.08, 0.62, 0.12, 0.55]),
    ("hunter", [0.30, 0.66, 0.18, 0.60]),
    ("bold", [0.45, 0.58, 0.30, 0.62]),
    ("grumpy", [-0.35, 0.40, 0.10, 0.22]),
    ("zoomy", [0.70, 0.92, 0.42, 0.80]),
    ("mellow", [0.30, 0.22, 0.40, 0.34]),
    ("clingy", [0.50, 0.40, 0.95, 0.50]),
    ("bashful", [-0.05, 0.30, 0.45, 0.40]),
    ("nope", [-0.22, 0.96, 0.04, 0.22]),
    ("drifter", [0.36, 0.78, 0.22, 0.70]),
];

/// `MOOD_BASELINES.get(name)`
pub fn mood_baseline(name: &str) -> Option<[f64; 4]> {
    let key = name.to_lowercase();
    MOOD_BASELINES.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

/// `Mood`
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mood {
    pub valence: f64,
    pub arousal: f64,
    pub affection: f64,
    pub curiosity: f64,
    pub base_valence: f64,
    pub base_arousal: f64,
    pub base_affection: f64,
    pub base_curiosity: f64,
}

impl Default for Mood {
    fn default() -> Self {
        Mood {
            valence: 0.18,
            arousal: 0.40,
            affection: 0.32,
            curiosity: 0.52,
            base_valence: 0.18,
            base_arousal: 0.40,
            base_affection: 0.32,
            base_curiosity: 0.52,
        }
    }
}

/// Keyword arguments of `Mood.bump`; unset fields are 0.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MoodBump {
    pub valence: f64,
    pub arousal: f64,
    pub affection: f64,
    pub curiosity: f64,
}

impl Mood {
    pub fn set_baseline(&mut self, valence: f64, arousal: f64, affection: f64, curiosity: f64) {
        self.base_valence = clamp(valence, -1.0, 1.0);
        self.base_arousal = clamp(arousal, 0.0, 1.0);
        self.base_affection = clamp(affection, 0.0, 1.0);
        self.base_curiosity = clamp(curiosity, 0.0, 1.0);
    }

    pub fn set_baseline_named(&mut self, name: &str) {
        if let Some([v, a, af, c]) = mood_baseline(name) {
            self.set_baseline(v, a, af, c);
        }
    }

    pub fn relax(&mut self, dt: f64, rate: f64) {
        self.valence = approach(self.valence, self.base_valence, dt, rate);
        self.arousal = approach(self.arousal, self.base_arousal, dt, rate * 0.85);
        self.affection = approach(self.affection, self.base_affection, dt, rate * 0.7);
        self.curiosity = approach(self.curiosity, self.base_curiosity, dt, rate * 0.9);
    }

    pub fn bump(&mut self, b: MoodBump) {
        self.valence = clamp(self.valence + b.valence, -1.0, 1.0);
        self.arousal = clamp(self.arousal + b.arousal, 0.0, 1.0);
        self.affection = clamp(self.affection + b.affection, 0.0, 1.0);
        self.curiosity = clamp(self.curiosity + b.curiosity, 0.0, 1.0);
    }

    pub fn happy(&self) -> f64 {
        clamp(self.valence, 0.0, 1.0)
    }

    pub fn sad(&self) -> f64 {
        clamp(-self.valence, 0.0, 1.0)
    }

    pub fn sleepy(&self) -> f64 {
        clamp((0.5 - self.arousal) * 2.0, 0.0, 1.0)
    }

    /// A coarse descriptor.
    pub fn label(&self) -> &'static str {
        if self.arousal < 0.22 && self.valence >= -0.1 {
            return "sleepy";
        }
        if self.affection > 0.7 && self.valence > 0.2 {
            return "affectionate";
        }
        if self.curiosity > 0.7 && self.arousal > 0.4 {
            return "curious";
        }
        if self.valence > 0.45 && self.arousal > 0.55 {
            return "playful";
        }
        if self.valence < -0.3 {
            return "glum";
        }
        if self.arousal > 0.7 {
            return "excited";
        }
        "content"
    }
}

/// `mood.bump(...)` shorthand: `bump!(mood, arousal = 0.3, valence = -0.25)`.
#[macro_export]
macro_rules! bump {
    ($mood:expr $(, $field:ident = $value:expr)* $(,)?) => {
        {
            #[allow(clippy::needless_update)]
            let b = $crate::mood::MoodBump { $($field: $value,)* ..Default::default() };
            $mood.bump(b)
        }
    };
}

/// `AntennaDrive`: resolved per-frame antenna parameters (units of size).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AntennaDrive {
    pub length: f64,
    pub base_angle: f64,
    pub curl: f64,
    pub curve_gain: f64,
    pub wave_amp: f64,
    pub wave_freq: f64,
    pub droopiness: f64,
    pub aim_angle: f64,
    pub aim_blend: f64,
    pub thickness: f64,
    pub tip_bulb: f64,
}

impl Default for AntennaDrive {
    fn default() -> Self {
        AntennaDrive {
            length: 1.0,
            base_angle: 0.62,
            curl: 0.4,
            curve_gain: 0.55,
            wave_amp: 0.25,
            wave_freq: 2.4,
            droopiness: 0.0,
            aim_angle: 0.0,
            aim_blend: 0.0,
            thickness: 1.0,
            tip_bulb: 1.0,
        }
    }
}

/// `antenna_drive_from_mood(mood, aiming=, inspecting=, cuddling=, aim_angle=)`
pub fn antenna_drive_from_mood(
    mood: &Mood,
    aiming: f64,
    inspecting: f64,
    cuddling: f64,
    aim_angle: f64,
) -> AntennaDrive {
    let happy = mood.happy();
    let sad = mood.sad();
    let sleepy = mood.sleepy();

    let mut curl = clamp(0.18 + mood.valence * 0.85 + mood.affection * 0.35 - sleepy * 0.30, -1.0, 1.0);
    let mut length = clamp(0.86 + mood.curiosity * 0.26 + mood.arousal * 0.12 - sleepy * 0.20, 0.6, 1.5);
    let mut wave_amp = clamp(0.12 + mood.curiosity * 0.45 + mood.arousal * 0.28 - sleepy * 0.10, 0.0, 1.1);
    let mut wave_freq = clamp(1.6 + mood.arousal * 3.4 + mood.curiosity * 1.2, 0.8, 7.0);
    let droop = clamp(sad * 0.8 + sleepy * 0.7, 0.0, 1.4);
    let base_angle = clamp(0.62 - mood.arousal * 0.10 + droop * 0.18, 0.30, 0.95);

    length += inspecting * 0.28;
    wave_amp += inspecting * 0.35;
    wave_freq += inspecting * 1.4;
    let mut aim_blend = clamp(inspecting * 0.62, 0.0, 0.85);

    let stiffen = clamp(aiming, 0.0, 1.0);
    curl = curl * (1.0 - stiffen * 0.75) + 0.10 * stiffen;
    wave_amp *= 1.0 - stiffen * 0.65;
    length += aiming * 0.18;
    aim_blend = aim_blend.max(clamp(aiming * 0.92, 0.0, 0.95));

    curl = curl * (1.0 - cuddling * 0.5) + 0.9 * cuddling;
    wave_amp = wave_amp * (1.0 - cuddling * 0.4) + 0.18 * cuddling;
    wave_freq = wave_freq * (1.0 - cuddling * 0.4) + 1.1 * cuddling;
    aim_blend = aim_blend.max(clamp(cuddling * 0.55, 0.0, 0.7));

    let thickness = clamp(0.92 + mood.arousal * 0.16 + cuddling * 0.12, 0.7, 1.4);
    let tip_bulb = clamp(0.85 + happy * 0.5 + cuddling * 0.5 + mood.curiosity * 0.2, 0.6, 1.9);

    AntennaDrive {
        length,
        base_angle,
        curl,
        curve_gain: 0.55,
        wave_amp,
        wave_freq,
        droopiness: droop,
        aim_angle,
        aim_blend,
        thickness,
        tip_bulb,
    }
}

/// `build_antenna_points(side_sign, n_seg, drive, wave_phase)`: body-local
/// `(forward, side)` points from base to tip, in `size` units.
pub fn build_antenna_points(side_sign: f64, n_seg: i64, drive: &AntennaDrive, wave_phase: f64) -> Vec<(f64, f64)> {
    let n_seg = n_seg.max(2) as usize;
    let base_angle = side_sign * drive.base_angle;
    let start_angle = base_angle * (1.0 - drive.aim_blend) + drive.aim_angle * drive.aim_blend;
    let eff_curve = drive.curve_gain * (1.0 - 0.8 * drive.aim_blend) * side_sign;
    let seg = drive.length / n_seg as f64;

    let mut pts = Vec::with_capacity(n_seg + 1);
    pts.push((0.0, 0.0));
    let mut fwd = 0.0;
    let mut side = 0.0;
    let mut ang = start_angle;
    for i in 0..n_seg {
        let t = i as f64 / (n_seg - 1) as f64;
        let wave = (wave_phase - i as f64 * drive.wave_freq * 0.55).sin()
            * drive.wave_amp
            * (0.30 + t)
            * (1.0 - 0.85 * drive.aim_blend)
            * side_sign;
        ang += eff_curve * drive.curl + wave * 0.5 + side_sign * drive.droopiness * 0.10 * t;
        fwd += ang.cos() * seg;
        side += ang.sin() * seg;
        pts.push((fwd, side));
    }
    pts
}
