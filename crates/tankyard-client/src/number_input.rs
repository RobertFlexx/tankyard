//! Bounded arithmetic for numeric controls. No script execution or side effects.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Error {
    Invalid,
    DivisionByZero,
    OutOfRange,
}
impl Error {
    pub fn message(self) -> &'static str {
        match self {
            Self::Invalid => "Enter a number or a simple calculation",
            Self::DivisionByZero => "Cannot divide by zero",
            Self::OutOfRange => "Value is outside the allowed range",
        }
    }
}

/// Evaluate in the field's displayed units. Relative edits use the value at
/// focus time, so retrying or repainting never compounds an operation.
pub fn evaluate(text: &str, original: f32, limits: [f32; 2], integer: bool) -> Result<f32, Error> {
    if text.len() > 128
        || !original.is_finite()
        || !limits.into_iter().all(f32::is_finite)
        || limits[0] > limits[1]
    {
        return Err(Error::Invalid);
    }
    let text = text.trim().trim_end_matches(['%', '°', '×']).trim();
    let (relative, expression) = match text.get(..2) {
        Some(op @ ("+=" | "-=" | "*=" | "/=")) => (Some(op), &text[2..]),
        _ => (None, text),
    };
    let mut parser = Parser {
        bytes: expression.as_bytes(),
        at: 0,
    };
    let mut value = parser.sum(0)?;
    parser.spaces();
    if parser.at != parser.bytes.len() {
        return Err(Error::Invalid);
    }
    let base = f64::from(original);
    value = match relative {
        Some("+=") => base + value,
        Some("-=") => base - value,
        Some("*=") => base * value,
        Some("/=") if value == 0. => return Err(Error::DivisionByZero),
        Some("/=") => base / value,
        _ => value,
    };
    if !value.is_finite() {
        return Err(Error::Invalid);
    }
    if integer {
        value = value.round();
    }
    if value < f64::from(limits[0]) || value > f64::from(limits[1]) {
        return Err(Error::OutOfRange);
    }
    Ok(value as f32)
}
struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl Parser<'_> {
    fn spaces(&mut self) {
        while self.bytes.get(self.at).is_some_and(u8::is_ascii_whitespace) {
            self.at += 1;
        }
    }
    fn take(&mut self, byte: u8) -> bool {
        self.spaces();
        if self.bytes.get(self.at) == Some(&byte) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn sum(&mut self, depth: u8) -> Result<f64, Error> {
        let mut value = self.product(depth)?;
        loop {
            if self.take(b'+') {
                value += self.product(depth)?;
            } else if self.take(b'-') {
                value -= self.product(depth)?;
            } else {
                return Ok(value);
            }
        }
    }
    fn product(&mut self, depth: u8) -> Result<f64, Error> {
        let mut value = self.atom(depth)?;
        loop {
            if self.take(b'*') {
                value *= self.atom(depth)?;
            } else if self.take(b'/') {
                let by = self.atom(depth)?;
                if by == 0. {
                    return Err(Error::DivisionByZero);
                }
                value /= by;
            } else {
                return Ok(value);
            }
        }
    }
    fn atom(&mut self, depth: u8) -> Result<f64, Error> {
        if depth > 12 {
            return Err(Error::Invalid);
        }
        if self.take(b'+') {
            return self.atom(depth + 1);
        }
        if self.take(b'-') {
            return self.atom(depth + 1).map(|v| -v);
        }
        if self.take(b'(') {
            let value = self.sum(depth + 1)?;
            return if self.take(b')') {
                Ok(value)
            } else {
                Err(Error::Invalid)
            };
        }
        self.spaces();
        let start = self.at;
        while self
            .bytes
            .get(self.at)
            .is_some_and(|b| b.is_ascii_digit() || *b == b'.')
        {
            self.at += 1;
        }
        if self.at == start {
            return Err(Error::Invalid);
        }
        if self
            .bytes
            .get(self.at)
            .is_some_and(|b| matches!(b, b'e' | b'E'))
        {
            self.at += 1;
            if self
                .bytes
                .get(self.at)
                .is_some_and(|b| matches!(b, b'+' | b'-'))
            {
                self.at += 1;
            }
            while self.bytes.get(self.at).is_some_and(u8::is_ascii_digit) {
                self.at += 1;
            }
        }
        std::str::from_utf8(&self.bytes[start..self.at])
            .ok()
            .and_then(|s| s.parse().ok())
            .ok_or(Error::Invalid)
    }
}

pub fn hex_color(text: &str) -> Option<[u8; 3]> {
    let value = text.trim().strip_prefix('#').unwrap_or(text.trim());
    if !value.is_ascii() {
        return None;
    }
    match value.len() {
        3 => {
            let mut rgb = [0; 3];
            for (out, digit) in rgb.iter_mut().zip(value.chars()) {
                *out = digit.to_digit(16)? as u8 * 17;
            }
            Some(rgb)
        }
        6 => Some([
            u8::from_str_radix(&value[0..2], 16).ok()?,
            u8::from_str_radix(&value[2..4], 16).ok()?,
            u8::from_str_radix(&value[4..6], 16).ok()?,
        ]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn precise_calculations_and_relative_changes_use_the_original() {
        for (text, wanted) in [
            ("1.375", 1.375),
            ("(2 + 3) * 12 / 4", 15.),
            ("+= 2*3", 16.),
            ("*=1.5", 15.),
            ("/=4", 2.5),
            ("-=2", 8.),
            ("1.25e1%", 12.5),
        ] {
            assert_eq!(
                evaluate(text, 10., [-100., 100.], false),
                Ok(wanted),
                "{text}"
            );
        }
        assert_eq!(evaluate("1250", 2400., [900., 16000.], true), Ok(1250.));
    }
    #[test]
    fn malformed_nonfinite_and_excessive_expressions_cannot_change_values() {
        for text in [
            "",
            "NaN",
            "inf",
            "1e300*1e300",
            "2+",
            "(1+2",
            "1..2",
            "2; print(1)",
            "π",
            "１２",
            "10,5",
            "1e",
            "((((((((((((((1))))))))))))))",
        ] {
            assert!(evaluate(text, 1., [-100., 100.], false).is_err(), "{text}");
        }
        assert_eq!(
            evaluate("5 / 0", 1., [0., 10.], false),
            Err(Error::DivisionByZero)
        );
        assert_eq!(evaluate("11", 1., [0., 10.], false), Err(Error::OutOfRange));
        assert_eq!(
            evaluate(&"1".repeat(129), 1., [0., 10.], false),
            Err(Error::Invalid)
        );
    }
    #[test]
    fn hex_colors_accept_short_and_full_codes_and_reject_bad_input() {
        assert_eq!(hex_color(" #aBc "), Some([170, 187, 204]));
        assert_eq!(hex_color("00b2e1"), Some([0, 178, 225]));
        for text in ["#", "#abcd", "ggg", "1234567", "é12345", "🟦"] {
            assert_eq!(hex_color(text), None);
        }
    }
}
