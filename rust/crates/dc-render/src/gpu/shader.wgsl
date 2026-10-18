// dc-render uber-shader: one pipeline layout for solid colours, gradients,
// images and glyphs. Output is premultiplied alpha.

struct Globals {
    size: vec2<f32>,
    pad: vec2<f32>,
};

struct Paint {
    kind: u32,          // low 4 bits: 0 solid, 1 linear, 2 radial, 3 conical, 4 image, 5 image (nearest); bit 4: glyph coverage
    stop_start: u32,
    stop_count: u32,
    z: f32,
    color: vec4<f32>,   // premultiplied solid colour, opacity applied
    inv0: vec4<f32>,    // device -> brush space: m11 m12 m21 m22
    inv1: vec4<f32>,    // dx dy opacity spread
    g0: vec4<f32>,      // linear: x1 y1 x2 y2 | radial: cx cy fx fy | conical: cx cy angle
    g1: vec4<f32>,      // radial: radius focal_radius
};

struct Stop {
    color: vec4<f32>,
    pos: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var<storage, read> paints: array<Paint>;
@group(0) @binding(2) var<storage, read> stops: array<Stop>;
@group(0) @binding(3) var glyph_tex: texture_2d<f32>;
@group(0) @binding(4) var lin_sampler: sampler;
@group(1) @binding(0) var image_tex: texture_2d<f32>;

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) local: vec2<f32>,
    @location(2) @interpolate(flat) paint: u32,
};

@vertex
fn vs_main(@location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) paint: u32) -> VOut {
    let p = paints[paint];
    var o: VOut;
    let ndc = vec2<f32>(pos.x / globals.size.x * 2.0 - 1.0, 1.0 - pos.y / globals.size.y * 2.0);
    o.pos = vec4<f32>(ndc, p.z, 1.0);
    o.uv = uv;
    o.local = vec2<f32>(
        p.inv0.x * pos.x + p.inv0.z * pos.y + p.inv1.x,
        p.inv0.y * pos.x + p.inv0.w * pos.y + p.inv1.y,
    );
    o.paint = paint;
    return o;
}

fn apply_spread(t: f32, mode: f32) -> f32 {
    if mode < 0.5 {
        return clamp(t, 0.0, 1.0);
    } else if mode < 1.5 {
        let m = t - 2.0 * floor(t * 0.5);
        return select(m, 2.0 - m, m > 1.0);
    }
    return fract(t);
}

fn eval_stops(p: Paint, t: f32) -> vec4<f32> {
    let n = p.stop_count;
    let s0 = stops[p.stop_start];
    if n <= 1u || t <= s0.pos.x {
        return s0.color;
    }
    var prev = s0;
    for (var i = 1u; i < n; i = i + 1u) {
        let s = stops[p.stop_start + i];
        if t <= s.pos.x {
            let span = s.pos.x - prev.pos.x;
            let f = select((t - prev.pos.x) / span, 1.0, span <= 0.0);
            return mix(prev.color, s.color, f);
        }
        prev = s;
    }
    return prev.color;
}

fn radial_t(p: Paint, q: vec2<f32>) -> vec2<f32> {
    // returns (t, valid)
    let c = p.g0.xy;
    let f = p.g0.zw;
    let r = p.g1.x;
    let fr = p.g1.y;
    let cd = c - f;
    let dr = r - fr;
    let pd = q - f;
    let a = dot(cd, cd) - dr * dr;
    let b = dot(pd, cd) + fr * dr;
    let cc = dot(pd, pd) - fr * fr;
    if abs(a) < 1e-6 {
        if abs(b) < 1e-9 {
            return vec2<f32>(0.0, 0.0);
        }
        let t = cc / (2.0 * b);
        return vec2<f32>(t, select(0.0, 1.0, fr + t * dr >= 0.0));
    }
    let det = b * b - a * cc;
    if det < 0.0 {
        return vec2<f32>(0.0, 0.0);
    }
    let sq = sqrt(det);
    let t1 = (b + sq) / a;
    let t2 = (b - sq) / a;
    let hi = max(t1, t2);
    let lo = min(t1, t2);
    if fr + hi * dr >= 0.0 {
        return vec2<f32>(hi, 1.0);
    }
    if fr + lo * dr >= 0.0 {
        return vec2<f32>(lo, 1.0);
    }
    return vec2<f32>(0.0, 0.0);
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let p = paints[in.paint];
    let kind = p.kind & 15u;
    let opacity = p.inv1.z;
    var c = p.color;
    if kind == 1u {
        let d = p.g0.zw - p.g0.xy;
        let l2 = dot(d, d);
        let t = select(dot(in.local - p.g0.xy, d) / l2, 0.0, l2 <= 0.0);
        c = eval_stops(p, apply_spread(t, p.inv1.w)) * opacity;
    } else if kind == 2u {
        let tv = radial_t(p, in.local);
        c = select(vec4<f32>(0.0), eval_stops(p, apply_spread(tv.x, p.inv1.w)) * opacity, tv.y > 0.5);
    } else if kind == 3u {
        let d = in.local - p.g0.xy;
        let a = atan2(-d.y, d.x) - p.g0.z;
        c = eval_stops(p, fract(a / 6.2831853)) * opacity;
    } else if kind == 4u {
        c = textureSampleLevel(image_tex, lin_sampler, in.uv, 0.0) * opacity;
    } else if kind == 5u {
        let dims = vec2<f32>(textureDimensions(image_tex));
        let texel = clamp(vec2<i32>(floor(in.uv * dims)), vec2<i32>(0), vec2<i32>(dims) - vec2<i32>(1));
        c = textureLoad(image_tex, texel, 0) * opacity;
    }
    if (p.kind & 16u) != 0u {
        c = c * textureSampleLevel(glyph_tex, lin_sampler, in.uv, 0.0).r;
    }
    return c;
}
