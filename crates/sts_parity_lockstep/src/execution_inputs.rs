use serde::{Deserialize, Serialize};

pub const EXECUTION_INPUTS_SCHEMA_V1: &str = "sts-lockstep-execution-inputs-v1";

/// Environmental inputs, not simulator outcomes. Each sample is an original
/// f32 read at the Act 3 Secret Portal candidate gate, in consumption order.
/// None means the source did not observe this contract; Some([]) proves that
/// the action did not read it. Missing evidence must never become time zero.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionInputsV1 {
    pub secret_portal_playtime_bits: Option<Vec<u32>>,
}

impl ExecutionInputsV1 {
    pub fn observed_empty() -> Self {
        Self {
            secret_portal_playtime_bits: Some(Vec::new()),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if let Some(samples) = &self.secret_portal_playtime_bits {
            for bits in samples {
                let time = f32::from_bits(*bits);
                if !time.is_finite() || time.is_sign_negative() {
                    return Err("Secret Portal playtime must be finite and nonnegative".to_owned());
                }
            }
        }
        Ok(())
    }

    pub fn has_samples(&self) -> bool {
        self.secret_portal_playtime_bits
            .as_ref()
            .is_some_and(|v| !v.is_empty())
    }

    /// Fold all raw actions, including mandatory UI settlement. A missing
    /// segment cannot erase observed samples or pretend to prove non-use.
    pub fn append(&mut self, next: Self) -> Result<(), String> {
        next.validate()?;
        match (
            &mut self.secret_portal_playtime_bits,
            next.secret_portal_playtime_bits,
        ) {
            (Some(current), Some(next)) => current.extend(next),
            (current, next) => {
                if current.as_ref().is_some_and(|v| !v.is_empty())
                    || next.as_ref().is_some_and(|v| !v.is_empty())
                {
                    return Err(
                        "external input evidence is incomplete across Java settlement".to_owned(),
                    );
                }
                *current = None;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_inputs_preserve_absence_order_and_reject_invalid_floats() {
        let sample = |time: f32| ExecutionInputsV1 {
            secret_portal_playtime_bits: Some(vec![time.to_bits()]),
        };
        for time in [f32::NAN, f32::INFINITY, -1.0, -0.0] {
            assert!(sample(time).validate().is_err());
        }
        let mut aggregate = ExecutionInputsV1::observed_empty();
        aggregate.append(sample(799.0)).unwrap();
        aggregate
            .append(ExecutionInputsV1::observed_empty())
            .unwrap();
        aggregate.append(sample(800.0)).unwrap();
        assert_eq!(
            aggregate.secret_portal_playtime_bits,
            Some(vec![799.0f32.to_bits(), 800.0f32.to_bits()])
        );
        assert!(aggregate.append(ExecutionInputsV1::default()).is_err());
        let mut absent = ExecutionInputsV1::default();
        absent.append(ExecutionInputsV1::observed_empty()).unwrap();
        assert_eq!(absent, ExecutionInputsV1::default());
        assert!(absent.append(sample(800.0)).is_err());
    }
}
