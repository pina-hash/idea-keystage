//! Lenient number deserializers for settings and config fields.
//!
//! The settings window is JavaScript, where every number is a float, so a
//! field the Rust side stores as an integer may arrive as `2200.0` or
//! `2200.4`. These accept any JSON number, round it, and clamp it into the
//! target type instead of failing the whole settings file.

use std::fmt;

use serde::de::{self, Deserializer, Visitor};

struct NumVisitor;

impl Visitor<'_> for NumVisitor {
    type Value = f64;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a number")
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<f64, E> {
        Ok(v as f64)
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<f64, E> {
        Ok(v as f64)
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<f64, E> {
        if v.is_finite() {
            Ok(v)
        } else {
            Err(E::custom("the number is not finite"))
        }
    }
}

fn number<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    d.deserialize_any(NumVisitor)
}

fn clamp_round(v: f64, max: f64) -> f64 {
    v.round().clamp(0.0, max)
}

pub(crate) fn u64_lenient<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    let v = number(d)?;
    // u64::MAX as f64 rounds up; 2^53 is far beyond any value we store.
    Ok(clamp_round(v, 9_007_199_254_740_992.0) as u64)
}

pub(crate) fn u32_lenient<'de, D: Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    let v = number(d)?;
    Ok(clamp_round(v, f64::from(u32::MAX)) as u32)
}

pub(crate) fn u16_lenient<'de, D: Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    let v = number(d)?;
    Ok(clamp_round(v, f64::from(u16::MAX)) as u16)
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct T {
        #[serde(deserialize_with = "super::u64_lenient")]
        a: u64,
        #[serde(deserialize_with = "super::u32_lenient")]
        b: u32,
        #[serde(deserialize_with = "super::u16_lenient")]
        c: u16,
    }

    #[test]
    fn accepts_any_number() {
        let t: T = serde_json::from_str(r#"{"a":2200.4,"b":-5,"c":70000}"#).unwrap();
        assert_eq!((t.a, t.b, t.c), (2200, 0, 65535));
        let t: T = serde_json::from_str(r#"{"a":7,"b":3.6,"c":47820}"#).unwrap();
        assert_eq!((t.a, t.b, t.c), (7, 4, 47820));
        assert!(serde_json::from_str::<T>(r#"{"a":"7","b":1,"c":1}"#).is_err());
    }
}
