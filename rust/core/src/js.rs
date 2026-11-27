//! JavaScript semantics the engine depends on: whitespace, trimming, UTF-16 lengths,
//! `Math.round` and `Number.prototype.toString`. Ported so output matches Mindloom 1.x exactly.

/// `\s` in a JavaScript regular expression.
pub fn is_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}' | '\u{2028}' | '\u{2029}' | '\u{202F}' | '\u{205F}' | '\u{3000}' | '\u{FEFF}'
    )
}

/// The same class written for the `regex` crate.
pub const WS: &str = r"[\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}]";

/// `String.prototype.trim`.
pub fn trim(s: &str) -> &str {
    s.trim_matches(is_space)
}
/// `s.replace(/\s+$/, '')`.
pub fn trim_end(s: &str) -> &str {
    s.trim_end_matches(is_space)
}
/// `s.replace(/^\s+/, '')`.
pub fn trim_start(s: &str) -> &str {
    s.trim_start_matches(is_space)
}

/// `s.length`: UTF-16 code units.
pub fn len16(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// `s.slice(0, n)` in UTF-16 units (a split surrogate pair is dropped rather than kept half).
pub fn slice16(s: &str, n: usize) -> &str {
    let mut units = 0;
    for (i, c) in s.char_indices() {
        units += c.len_utf16();
        if units > n {
            return &s[..i];
        }
    }
    s
}

/// `Math.round`: halves round towards +∞.
pub fn round(x: f64) -> f64 {
    let f = x.floor();
    if x - f >= 0.5 {
        f + 1.0
    } else {
        f
    }
}

/// `Math.round(v * 10) / 10`, the one-decimal rounding used in SVG output.
pub fn r1(v: f64) -> f64 {
    round(v * 10.0) / 10.0
}

/// `Math.sign(v) || 1`.
pub fn sign1(v: f64) -> f64 {
    if v < 0.0 {
        -1.0
    } else {
        1.0
    }
}

/// `String(number)`.
pub fn num(v: f64) -> String {
    if v.is_nan() {
        return "NaN".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if v == 0.0 {
        return "0".into();
    }
    let a = v.abs();
    if !(1e-6..1e21).contains(&a) {
        let s = format!("{v:e}");
        return match s.split_once('e') {
            Some((m, e)) if !e.starts_with('-') => format!("{m}e+{e}"),
            _ => s,
        };
    }
    format!("{v}")
}

/// `n(v)` in the renderer: one decimal, printed like JavaScript.
pub fn n(v: f64) -> String {
    num(r1(v))
}

/// Truthiness of an optional string (`""` is false).
pub fn some_str(s: &Option<String>) -> Option<&str> {
    s.as_deref().filter(|x| !x.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numbers_print_like_js() {
        assert_eq!(num(2.0), "2");
        assert_eq!(num(-0.0), "0");
        assert_eq!(num(3.5999999999999996), "3.5999999999999996");
        assert_eq!(num(0.1 + 0.2), "0.30000000000000004");
        assert_eq!(num(1e21), "1e+21");
        assert_eq!(num(1.5e-7), "1.5e-7");
        assert_eq!(n(-0.04), "0");
        assert_eq!(round(-2.5), -2.0);
        assert_eq!(round(2.5), 3.0);
    }
    #[test]
    fn utf16() {
        assert_eq!(len16("a😀"), 3);
        assert_eq!(slice16("a😀b", 2), "a");
        assert_eq!(slice16("a😀b", 3), "a😀");
        assert_eq!(trim("\u{FEFF} x \u{3000}"), "x");
    }
}
