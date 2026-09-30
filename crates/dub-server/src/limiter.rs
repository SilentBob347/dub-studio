//! Лимитер пиков фразы дубляжа — перенос audio_post::limiter студий (лимитер Hyrax из matchering).
//! Обработка офлайн, поэтому «предпросмотр» даёт centred_max + filtfilt с нулевой фазой: гейн начинает
//! опускаться ДО пика, а не срезает его. Жёсткий гейн (hard) входит в итоговый максимум, так что ни один
//! сэмпл не выходит за порог. Моно-вариант `limit_mono` и речевой пресет `SPEECH` — адаптация под дубляж.

#[derive(Debug, Clone, Copy)]
pub struct LimiterConfig {
    pub attack_ms: f64,
    pub hold_ms: f64,
    pub release_ms: f64,
    pub attack_filter_coefficient: f64,
    pub hold_filter_coefficient: f64,
    pub release_filter_coefficient: f64,
}

impl Default for LimiterConfig {
    fn default() -> Self {
        Self {
            attack_ms: 1.0,
            hold_ms: 1.0,
            release_ms: 3000.0,
            attack_filter_coefficient: -2.0,
            hold_filter_coefficient: 7.0,
            release_filter_coefficient: 800.0,
        }
    }
}

/// Речевой пресет. Срез release-фильтра = release_filter_coefficient / release_ms Гц: 800/400 = 2 Гц,
/// постоянная времени ≈ 80 мс — гейн возвращается между слогами, а не давит всю фразу после одного
/// пика (как 3000 мс мастеринга музыки). Атака 5 мс ловит взрывные согласные, hold 10 мс держит гейн
/// на пике периода голоса и не даёт модуляции на низких частотах.
pub const SPEECH: LimiterConfig = LimiterConfig {
    attack_ms: 5.0,
    hold_ms: 10.0,
    release_ms: 400.0,
    attack_filter_coefficient: -2.0,
    hold_filter_coefficient: 7.0,
    release_filter_coefficient: 800.0,
};

fn ms_to_samples(ms: f64, rate: u32) -> usize {
    (rate as f64 * ms * 1e-3) as usize
}

/// Maximum over a centred window of `size` (odd), edges clipped.
fn centred_max(x: &[f64], size: usize) -> Vec<f64> {
    let half = size / 2;
    let n = x.len();
    let mut out = vec![0.0; n];
    let mut deque: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
    let mut next = 0;
    for (i, slot) in out.iter_mut().enumerate() {
        let hi = (i + half).min(n - 1);
        while next <= hi {
            while deque.back().is_some_and(|&b| x[b] <= x[next]) {
                deque.pop_back();
            }
            deque.push_back(next);
            next += 1;
        }
        let lo = i.saturating_sub(half);
        while deque.front().is_some_and(|&f| f < lo) {
            deque.pop_front();
        }
        *slot = x[*deque.front().unwrap()];
    }
    out
}

/// Maximum over the `size` samples ending at each one.
fn trailing_max(x: &[f64], size: usize) -> Vec<f64> {
    let mut out = vec![0.0; x.len()];
    let mut deque: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
    for i in 0..x.len() {
        while deque.back().is_some_and(|&b| x[b] <= x[i]) {
            deque.pop_back();
        }
        deque.push_back(i);
        while deque.front().is_some_and(|&f| f + size <= i) {
            deque.pop_front();
        }
        // the window reaches before the start, where matchering pads with zeros
        out[i] = x[*deque.front().unwrap()].max(0.0);
    }
    out
}

/// First-order Butterworth low-pass, bilinear with pre-warping (SciPy `butter(1, fc, fs=rate)`).
fn butter1(cutoff: f64, rate: u32) -> ([f64; 2], [f64; 2]) {
    let k = (std::f64::consts::PI * cutoff / rate as f64).tan();
    let b = k / (1.0 + k);
    ([b, b], [1.0, (k - 1.0) / (k + 1.0)])
}

/// `lfilter` for a first-order section, from rest.
fn lfilter1(b: [f64; 2], a: [f64; 2], x: &[f64]) -> Vec<f64> {
    let mut y = Vec::with_capacity(x.len());
    let (mut x1, mut y1) = (0.0, 0.0);
    for &xi in x {
        let yi = b[0] * xi + b[1] * x1 - a[1] * y1;
        y.push(yi);
        x1 = xi;
        y1 = yi;
    }
    y
}

/// SciPy `filtfilt(b=[1-c], a=[1,-c], x)`: odd extension of 6 samples at both
/// ends, steady-state initial conditions, forward then backward.
fn filtfilt_one_pole(c: f64, x: &[f64]) -> Vec<f64> {
    let n = x.len();
    let pad = 6.min(n.saturating_sub(1));
    let mut ext = Vec::with_capacity(n + 2 * pad);
    for i in (1..=pad).rev() {
        ext.push(2.0 * x[0] - x[i]);
    }
    ext.extend_from_slice(x);
    for i in 1..=pad {
        ext.push(2.0 * x[n - 1] - x[n - 1 - i]);
    }
    let b0 = 1.0 - c;
    let run = |input: &[f64]| -> Vec<f64> {
        // lfilter_zi of this section is c: a unit step settles at 1 from sample one
        let mut state = c * input[0];
        input
            .iter()
            .map(|&xi| {
                let yi = b0 * xi + state;
                state = c * yi;
                yi
            })
            .collect()
    };
    let forward = run(&ext);
    let reversed: Vec<f64> = forward.into_iter().rev().collect();
    let backward = run(&reversed);
    let mut y: Vec<f64> = backward.into_iter().rev().collect();
    y.drain(..pad);
    y.truncate(n);
    y
}

/// Кривая гейна (множитель 0..1 на сэмпл) по выпрямленному пику `peak[i]`. None — ни один сэмпл не
/// выше порога, сигнал не трогаем.
fn gain_curve(peak: &[f64], rate: u32, threshold: f64, config: &LimiterConfig) -> Option<Vec<f32>> {
    let rectified: Vec<f64> = peak.iter().map(|&p| p.max(threshold) / threshold).collect();
    if rectified.iter().all(|&r| (r - 1.0).abs() <= 1e-8 + 1e-5) {
        return None;
    }
    let hard: Vec<f64> = rectified.iter().map(|&r| 1.0 - 1.0 / r).collect();

    let attack = ms_to_samples(config.attack_ms, rate).max(1);
    let odd = if attack.is_multiple_of(2) { attack + 1 } else { attack };
    let slided = centred_max(&hard, 2 * odd - 1);
    let c = (config.attack_filter_coefficient / attack as f64).exp();
    let gain_attack = filtfilt_one_pole(c, &slided);

    let hold = ms_to_samples(config.hold_ms, rate).max(1);
    let held = trailing_max(&slided, hold);
    let (b, a) = butter1(config.hold_filter_coefficient, rate);
    let hold_out = lfilter1(b, a, &held);
    let (b, a) = butter1(config.release_filter_coefficient / config.release_ms, rate);
    let release_in: Vec<f64> = held.iter().zip(&hold_out).map(|(&h, &o)| h.max(o)).collect();
    let release_out = lfilter1(b, a, &release_in);

    Some(
        (0..peak.len())
            .map(|i| (1.0 - hard[i].max(gain_attack[i]).max(hold_out[i].max(release_out[i]))) as f32)
            .collect(),
    )
}

/// Моно-вариант студийного `limit`: выпрямленный пик берётся по одному каналу, остальное то же. Возвращает число
/// сэмплов, которые без лимитера вышли бы за порог (телеметрия для журнала рендера).
pub fn limit_mono(x: &mut [f32], rate: u32, threshold: f64, config: &LimiterConfig) -> usize {
    if x.is_empty() {
        return 0;
    }
    let over = x.iter().filter(|v| v.abs() as f64 > threshold).count();
    let peak: Vec<f64> = x.iter().map(|v| v.abs() as f64).collect();
    if let Some(gain) = gain_curve(&peak, rate, threshold, config) {
        for (v, g) in x.iter_mut().zip(gain) {
            *v *= g;
        }
    }
    over
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_changes_below_the_threshold() {
        let mut l = vec![0.5f32; 1000];
        limit_mono(&mut l, 48_000, 0.998, &LimiterConfig::default());
        assert!(l.iter().all(|&v| v == 0.5));
    }

    #[test]
    fn peaks_come_down_to_the_threshold() {
        let n = 48_000;
        let mut l: Vec<f32> = (0..n).map(|i| 1.6 * ((i as f32) * 0.05).sin()).collect();
        limit_mono(&mut l, 48_000, 0.998, &LimiterConfig::default());
        let peak = l.iter().fold(0.0f32, |p, v| p.max(v.abs()));
        assert!(peak <= 0.999, "peak {peak}");
        assert!(peak > 0.8, "limited too hard: {peak}");
    }

    #[test]
    fn filtfilt_keeps_a_constant() {
        let x = vec![0.25; 300];
        let y = filtfilt_one_pole(0.96, &x);
        assert!(y.iter().all(|v| (v - 0.25).abs() < 1e-9));
    }

    const SR: u32 = 24_000;
    const PLOSIVE_AT: f32 = 1.0625;
    const PLOSIVE_LEN: f32 = 0.012;

    /// Речеподобная фраза 2 с: тон 180 Гц со слоговой огибающей 4 Гц, пик огибающей 0.5, и одна
    /// взрывная согласная — гладкий (ханн 12 мс) всплеск x`burst` на пике слога.
    fn phrase(amp: f32, burst: f32) -> Vec<f32> {
        (0..SR as usize * 2)
            .map(|i| {
                let t = i as f32 / SR as f32;
                let syll = 0.55 + 0.45 * (2.0 * std::f32::consts::PI * 4.0 * t).sin();
                let mut v = amp * syll * (2.0 * std::f32::consts::PI * 180.0 * t).sin();
                let ph = t - PLOSIVE_AT;
                if (0.0..PLOSIVE_LEN).contains(&ph) {
                    let h = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * ph / PLOSIVE_LEN).cos();
                    v *= 1.0 + (burst - 1.0) * h;
                }
                v
            })
            .collect()
    }

    fn rms_of(x: &[f32], keep: impl Fn(f32) -> bool) -> f64 {
        let (mut sum, mut n) = (0.0f64, 0usize);
        for (i, &v) in x.iter().enumerate() {
            if keep(i as f32 / SR as f32) {
                sum += (v as f64) * (v as f64);
                n += 1;
            }
        }
        (sum / n.max(1) as f64).sqrt()
    }

    #[test]
    fn speech_limiter_leaves_no_overs() {
        let mut x = phrase(0.5, 4.0); // всплеск до ~1.97 = +6 дБ над полкой 0.985
        let over = limit_mono(&mut x, SR, 0.985, &SPEECH);
        assert!(over > 0, "в тестовой фразе должны быть перегрузки");
        let peak = x.iter().fold(0.0f32, |p, v| p.max(v.abs()));
        assert!(peak <= 0.985 + 1e-6, "перегрузка после лимитера: {peak}");
    }

    #[test]
    fn speech_limiter_does_not_touch_a_quiet_phrase() {
        let orig = phrase(0.3, 1.0);
        let mut x = orig.clone();
        let over = limit_mono(&mut x, SR, 0.985, &SPEECH);
        assert_eq!(over, 0);
        assert_eq!(x, orig, "тихая фраза должна пройти без изменений");
    }

    #[test]
    fn speech_limiter_keeps_the_phrase_loudness() {
        let orig = phrase(0.5, 4.0);
        let mut x = orig.clone();
        limit_mono(&mut x, SR, 0.985, &SPEECH);
        // Вне всплеска и хвоста release (200 мс) фраза звучит как была.
        let body = |t: f32| !(PLOSIVE_AT - 0.02..PLOSIVE_AT + 0.2).contains(&t);
        let body_db = 20.0 * (rms_of(&x, body) / rms_of(&orig, body)).log10();
        assert!(body_db.abs() < 0.1, "лимитер изменил громкость фразы вне пика: {body_db:.3} дБ");
        // Целиком фраза теряет только энергию самого пика.
        let all_db = 20.0 * (rms_of(&x, |_| true) / rms_of(&orig, |_| true)).log10();
        assert!(all_db > -0.5, "лимитер съел громкость фразы: {all_db:.2} дБ");
    }

    #[test]
    fn speech_limiter_gain_has_no_steps() {
        // Щелчок = скачок гейна между соседними сэмплами. У жёсткого клипа гейн на пике прыгает на
        // каждом сэмпле; у лимитера кривая гладкая.
        let orig = phrase(0.5, 4.0);
        let peak: Vec<f64> = orig.iter().map(|v| v.abs() as f64).collect();
        let gain = gain_curve(&peak, SR, 0.985, &SPEECH).expect("есть перегрузки");
        let max_step = gain.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
        assert!(max_step < 0.02, "скачок гейна лимитера {max_step}");
        let clip_gain: Vec<f64> = peak.iter().map(|&p| 0.985 / p.max(0.985)).collect();
        let clip_steps = clip_gain.windows(2).filter(|w| (w[1] - w[0]).abs() > 0.02).count();
        assert!(clip_steps > 0, "жёсткий клип на этой фразе должен давать скачки гейна");
    }
}
