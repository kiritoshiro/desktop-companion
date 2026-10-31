//! The antenna helpers the painter needs from `creature/mood.py`:
//! `AntennaDrive`, `antenna_drive_from_mood` and `build_antenna_points`.
//! Pure functions of the mood and a few situational weights, so they live
//! with the painter rather than behind a dependency on `dc-creature`.

use crate::draw::Mood;
use dc_core::math::clamp;

/// `AntennaDrive`: resolved per-frame antenna parameters (body-size units).
#[derive(Clone, Copy, Debug, PartialEq)]
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

/// `antenna_drive_from_mood(mood, aiming=, inspecting=, cuddling=, aim_angle=)`.
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
pub fn build_antenna_points(side_sign: f64, n_seg: usize, drive: &AntennaDrive, wave_phase: f64) -> Vec<(f64, f64)> {
    let n_seg = n_seg.max(2);
    let base_angle = side_sign * drive.base_angle;
    let start_angle = base_angle * (1.0 - drive.aim_blend) + drive.aim_angle * drive.aim_blend;
    let eff_curve = drive.curve_gain * (1.0 - 0.8 * drive.aim_blend) * side_sign;
    let seg = drive.length / n_seg as f64;
    let mut pts = Vec::with_capacity(n_seg + 1);
    pts.push((0.0, 0.0));
    let (mut fwd, mut side, mut ang) = (0.0, 0.0, start_angle);
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
