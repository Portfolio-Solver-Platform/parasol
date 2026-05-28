use super::{Ai as AiTrait, Error, Features, Result};
use crate::scheduler::{Portfolio, SolverInfo};

const K1_BYTES: &[u8] = include_bytes!("svc/data/svc_k1.bin");
const EK1_BYTES: &[u8] = include_bytes!("svc/data/svc_ek1.bin");

const CP_SAT: &str = "cp-sat";

const K1_PORTFOLIO: &[(&str, usize)] = &[
    (CP_SAT, 1),
    ("org.chuffed.chuffed", 1),
    ("org.gecode.gecode", 2),
    ("org.minizinc.mip.gurobi", 2),
    ("org.picat-lang.picat", 1),
    ("yuck", 1),
];

const EK1_PORTFOLIO: &[(&str, usize)] = &[
    (CP_SAT, 1),
    ("dexter", 1),
    ("nl.tudelft.algorithmics.pumpkin", 1),
    ("org.choco.choco", 1),
    ("org.minizinc.mip.gurobi", 2),
    ("org.picat-lang.picat", 1),
    ("yuck", 1),
];

pub struct SvcAi {
    bag: BagSvc,
    portfolio: &'static [(&'static str, usize)],
}

impl SvcAi {
    pub fn k1() -> Result<Self> {
        Ok(Self {
            bag: BagSvc::from_bytes(K1_BYTES)
                .map_err(|e| Error::Other(format!("svc_k1 model load: {e}")))?,
            portfolio: K1_PORTFOLIO,
        })
    }

    pub fn ek1() -> Result<Self> {
        Ok(Self {
            bag: BagSvc::from_bytes(EK1_BYTES)
                .map_err(|e| Error::Other(format!("svc_ek1 model load: {e}")))?,
            portfolio: EK1_PORTFOLIO,
        })
    }
}

impl AiTrait for SvcAi {
    fn schedule(&mut self, features: &Features, _cores: usize) -> Result<Portfolio> {
        if features.len() != self.bag.n_features {
            return Err(Error::Other(format!(
                "SVC AI expected {} features, got {}",
                self.bag.n_features,
                features.len()
            )));
        }
        let predicted = self.bag.predict(features);
        let entries = if predicted == 0 {
            vec![SolverInfo::new(CP_SAT.to_string(), 8)]
        } else {
            self.portfolio
                .iter()
                .map(|(id, c)| SolverInfo::new((*id).to_string(), *c))
                .collect()
        };
        Ok(entries)
    }
}

pub struct BagSvc {
    pub n_features: usize,
    n_svcs: usize,
    scaler_mean: Vec<f64>,
    scaler_scale: Vec<f64>,
    gamma: Vec<f64>,
    intercept: Vec<f64>,
    prob_a: Vec<f64>,
    prob_b: Vec<f64>,
    sv_offsets: Vec<usize>,
    support_vectors: Vec<f64>,
    dual_coef: Vec<f64>,
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("buffer too short: expected at least {expected} bytes, got {got}")]
    TooShort { expected: usize, got: usize },
    #[error("bad magic: expected b\"SVCB\", got {0:?}")]
    BadMagic([u8; 4]),
    #[error("unsupported version: {0}")]
    BadVersion(u32),
    #[error("trailing bytes after parsing: {0}")]
    TrailingBytes(usize),
}

impl BagSvc {
    pub fn from_bytes(bytes: &[u8]) -> std::result::Result<Self, LoadError> {
        let mut r = Reader::new(bytes);
        let magic = r.take_array::<4>()?;
        if magic != *b"SVCB" {
            return Err(LoadError::BadMagic(magic));
        }
        let version = r.read_u32()?;
        if version != 1 {
            return Err(LoadError::BadVersion(version));
        }
        let n_features = r.read_u32()? as usize;
        let n_svcs = r.read_u32()? as usize;
        let total_svs = r.read_u64()? as usize;

        let scaler_mean = r.read_f64_vec(n_features)?;
        let scaler_scale = r.read_f64_vec(n_features)?;
        let gamma = r.read_f64_vec(n_svcs)?;
        let intercept = r.read_f64_vec(n_svcs)?;
        let prob_a = r.read_f64_vec(n_svcs)?;
        let prob_b = r.read_f64_vec(n_svcs)?;

        let mut sv_offsets = Vec::with_capacity(n_svcs + 1);
        sv_offsets.push(0usize);
        let mut acc = 0usize;
        for _ in 0..n_svcs {
            acc += r.read_u32()? as usize;
            sv_offsets.push(acc);
        }
        assert_eq!(
            acc, total_svs,
            "sv_counts sum mismatches total_svs in header"
        );

        let support_vectors = r.read_f64_vec(total_svs * n_features)?;
        let dual_coef = r.read_f64_vec(total_svs)?;

        if !r.is_empty() {
            return Err(LoadError::TrailingBytes(r.remaining()));
        }

        Ok(Self {
            n_features,
            n_svcs,
            scaler_mean,
            scaler_scale,
            gamma,
            intercept,
            prob_a,
            prob_b,
            sv_offsets,
            support_vectors,
            dual_coef,
        })
    }

    pub fn predict(&self, features: &[f32]) -> usize {
        let [p0, p1] = self.predict_proba(features);
        if p0 >= p1 { 0 } else { 1 }
    }

    pub fn predict_proba(&self, features: &[f32]) -> [f64; 2] {
        assert_eq!(features.len(), self.n_features);
        let x: Vec<f64> = (0..self.n_features)
            .map(|i| {
                let v = features[i] as f64;
                let s = v.signum() * v.abs().ln_1p();
                (s - self.scaler_mean[i]) / self.scaler_scale[i]
            })
            .collect();

        let mut sum_p0 = 0.0;
        for i in 0..self.n_svcs {
            let a = self.sv_offsets[i];
            let b = self.sv_offsets[i + 1];
            let gamma = self.gamma[i];

            let mut decision = self.intercept[i];
            for j in a..b {
                let row = &self.support_vectors[j * self.n_features..(j + 1) * self.n_features];
                let mut sq = 0.0;
                for k in 0..self.n_features {
                    let d = x[k] - row[k];
                    sq += d * d;
                }
                decision += self.dual_coef[j] * (-gamma * sq).exp();
            }

            let q0 = platt(decision, self.prob_a[i], self.prob_b[i]);
            sum_p0 += multiclass_prob_binary(q0);
        }
        let mean_p0 = sum_p0 / self.n_svcs as f64;
        [mean_p0, 1.0 - mean_p0]
    }
}

// sklearn flips decision sign for binary SVC vs libsvm; probA/probB are stored
// in libsvm convention, so negate decision back before applying Platt.
fn platt(decision: f64, a: f64, b: f64) -> f64 {
    let f_ap_b = -decision * a + b;
    let p = if f_ap_b >= 0.0 {
        let e = (-f_ap_b).exp();
        e / (1.0 + e)
    } else {
        1.0 / (1.0 + f_ap_b.exp())
    };
    p.clamp(1e-7, 1.0 - 1e-7)
}

// libsvm runs Wu-Lin-Weng pairwise coupling even for k=2 with eps=0.0025;
// samples within ~0.005 of 0.5 exit at iter 0 and return exactly 0.5.
fn multiclass_prob_binary(q0: f64) -> f64 {
    let q1 = 1.0 - q0;
    let q00 = q1 * q1;
    let q11 = q0 * q0;
    let q01 = -q0 * q1;
    let mut p0 = 0.5_f64;
    let mut p1 = 0.5_f64;
    const EPS: f64 = 0.005 / 2.0;
    for _ in 0..100 {
        let qp0 = q00 * p0 + q01 * p1;
        let qp1 = q01 * p0 + q11 * p1;
        let pqp = p0 * qp0 + p1 * qp1;
        let max_err = (qp0 - pqp).abs().max((qp1 - pqp).abs());
        if max_err < EPS {
            break;
        }

        let diff = (-qp0 + pqp) / q00;
        let one_plus = 1.0 + diff;
        let pqp_b = (pqp + diff * (diff * q00 + 2.0 * qp0)) / (one_plus * one_plus);
        let qp1_b = (qp1 + diff * q01) / one_plus;
        let p0_b = (p0 + diff) / one_plus;
        let p1_b = p1 / one_plus;

        let diff = (-qp1_b + pqp_b) / q11;
        let one_plus = 1.0 + diff;
        p1 = (p1_b + diff) / one_plus;
        p0 = p0_b / one_plus;
    }
    p0
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    fn is_empty(&self) -> bool {
        self.pos == self.buf.len()
    }

    fn take(&mut self, n: usize) -> std::result::Result<&'a [u8], LoadError> {
        if self.pos + n > self.buf.len() {
            return Err(LoadError::TooShort {
                expected: self.pos + n,
                got: self.buf.len(),
            });
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn take_array<const N: usize>(&mut self) -> std::result::Result<[u8; N], LoadError> {
        Ok(self.take(N)?.try_into().unwrap())
    }

    fn read_u32(&mut self) -> std::result::Result<u32, LoadError> {
        Ok(u32::from_le_bytes(self.take_array::<4>()?))
    }

    fn read_u64(&mut self) -> std::result::Result<u64, LoadError> {
        Ok(u64::from_le_bytes(self.take_array::<8>()?))
    }

    fn read_f64_vec(&mut self, n: usize) -> std::result::Result<Vec<f64>, LoadError> {
        let bytes = self.take(n * 8)?;
        let mut out = Vec::with_capacity(n);
        for chunk in bytes.chunks_exact(8) {
            out.push(f64::from_le_bytes(chunk.try_into().unwrap()));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROBA_TOL: f64 = 1e-12;

    const K1_FIXTURE: &[u8] = include_bytes!("svc/data/svc_k1_fixture.bin");
    const EK1_FIXTURE: &[u8] = include_bytes!("svc/data/svc_ek1_fixture.bin");

    struct Fixture {
        n_features: usize,
        features: Vec<f32>,
        expected_probs: Vec<f64>,
        expected_class: Vec<u32>,
    }

    impl Fixture {
        fn n_samples(&self) -> usize {
            self.expected_class.len()
        }

        fn sample(&self, i: usize) -> (&[f32], [f64; 2], u32) {
            let f = &self.features[i * self.n_features..(i + 1) * self.n_features];
            let p = [self.expected_probs[i * 2], self.expected_probs[i * 2 + 1]];
            (f, p, self.expected_class[i])
        }
    }

    fn parse_fixture(bytes: &[u8]) -> Fixture {
        assert_eq!(&bytes[0..4], b"SVCF", "bad fixture magic");
        let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        assert_eq!(version, 1);
        let n_samples = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        let n_features = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;

        let mut off = 16;
        let features: Vec<f32> = bytes[off..off + n_samples * n_features * 4]
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        off += n_samples * n_features * 4;

        let expected_probs: Vec<f64> = bytes[off..off + n_samples * 2 * 8]
            .chunks_exact(8)
            .map(|c| f64::from_le_bytes(c.try_into().unwrap()))
            .collect();
        off += n_samples * 2 * 8;

        let expected_class: Vec<u32> = bytes[off..off + n_samples * 4]
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        off += n_samples * 4;
        assert_eq!(off, bytes.len(), "trailing bytes in fixture");

        Fixture {
            n_features,
            features,
            expected_probs,
            expected_class,
        }
    }

    fn check(name: &str, model_bytes: &[u8], fixture_bytes: &[u8]) {
        let bag = BagSvc::from_bytes(model_bytes).expect("model parse");
        let fix = parse_fixture(fixture_bytes);
        assert_eq!(fix.n_features, bag.n_features);

        let mut max_diff = 0.0_f64;
        let mut class_mismatches = 0usize;
        for i in 0..fix.n_samples() {
            let (features, expected_proba, expected_class) = fix.sample(i);
            let proba = bag.predict_proba(features);
            let diff = (proba[0] - expected_proba[0])
                .abs()
                .max((proba[1] - expected_proba[1]).abs());
            if diff > max_diff {
                max_diff = diff;
            }
            if bag.predict(features) as u32 != expected_class {
                class_mismatches += 1;
                if class_mismatches < 5 {
                    eprintln!(
                        "[{name}] row {i}: rust_proba={proba:?} expected_proba={expected_proba:?} expected_class={expected_class}"
                    );
                }
            }
        }
        eprintln!(
            "[{name}] {} rows, max|Δproba| = {:.2e}, class mismatches = {}",
            fix.n_samples(),
            max_diff,
            class_mismatches
        );
        assert!(
            max_diff < PROBA_TOL,
            "[{name}] max|Δproba| {max_diff:.2e} exceeds tolerance {PROBA_TOL:.0e}"
        );
        assert_eq!(class_mismatches, 0);
    }

    #[test]
    fn svc_k1_matches_python_fixture() {
        check("k1", K1_BYTES, K1_FIXTURE);
    }

    #[test]
    fn svc_ek1_matches_python_fixture() {
        check("ek1", EK1_BYTES, EK1_FIXTURE);
    }
}
