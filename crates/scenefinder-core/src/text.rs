//! Text rendering of reports, byte-for-byte compatible with the command-line
//! scene finder's output (Python-style number and string formatting).

use crate::report::RunReport;

/// `[mm:ss.ss]` from seconds, using Python's float `divmod` semantics.
pub fn clock(ts: f64) -> String {
    let (mm, ss) = py_divmod(ts, 60.0);
    format!("[{:02}:{:05.2}]", mm as i64, ss)
}

fn py_divmod(vx: f64, wx: f64) -> (f64, f64) {
    let mut m = vx % wx;
    let mut div = (vx - m) / wx;
    if m != 0.0 {
        if (wx < 0.0) != (m < 0.0) {
            m += wx;
            div -= 1.0;
        }
    } else {
        m = 0.0f64.copysign(wx);
    }
    let floordiv = if div != 0.0 {
        let mut f = div.floor();
        if div - f > 0.5 {
            f += 1.0;
        }
        f
    } else {
        0.0f64.copysign(vx / wx)
    };
    (floordiv, m)
}

/// Python's `%g`: six significant digits, trailing zeros removed, scientific
/// notation when the exponent is below -4 or at least 6.
pub fn fmt_g(x: f64) -> String {
    if x == 0.0 {
        return if x.is_sign_negative() {
            "-0".into()
        } else {
            "0".into()
        };
    }
    if !x.is_finite() {
        return if x.is_nan() {
            "nan".into()
        } else if x > 0.0 {
            "inf".into()
        } else {
            "-inf".into()
        };
    }
    let sci = format!("{:.5e}", x);
    let (mant, exp) = sci.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    if !(-4..6).contains(&exp) {
        let mant = strip_zeros(mant);
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{mant}e{sign}{:02}", exp.abs())
    } else {
        let decimals = (5 - exp).max(0) as usize;
        strip_zeros(&format!("{:.*}", decimals, x))
    }
}

fn strip_zeros(s: &str) -> String {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s.to_string()
    }
}

/// Python's `repr()` of a string.
pub fn py_repr(s: &str) -> String {
    let quote = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02x}", c as u32))
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// The complete command-line output (stdout and stderr interleaved in print
/// order) for a run.
pub fn render_report(r: &RunReport) -> String {
    let mut s = String::new();
    s.push_str(&r.header_line);
    s.push('\n');
    for d in &r.demos {
        if r.bulk && d.scene_count.unwrap_or(0) == 0 {
            continue;
        }
        s.push('\n');
        for line in &d.out_lines {
            s.push_str(line);
            s.push('\n');
        }
        for line in &d.err_lines {
            s.push_str(line);
            s.push('\n');
        }
    }
    if r.bulk {
        s.push_str(&format!(
            "\n# {} scene(s) across {}/{} demo(s)\n",
            r.total_scenes, r.demos_with_player, r.demo_count
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_matches_python() {
        assert_eq!(clock(176.95), "[02:56.95]");
        assert_eq!(clock(171.125), "[02:51.12]");
        assert_eq!(clock(0.375), "[00:00.38]");
        assert_eq!(clock(59.996), "[00:60.00]");
        assert_eq!(clock(0.0), "[00:00.00]");
        assert_eq!(clock(3600.5), "[60:00.50]");
    }

    #[test]
    fn g_matches_python() {
        assert_eq!(fmt_g(5.0), "5");
        assert_eq!(fmt_g(0.25), "0.25");
        assert_eq!(fmt_g(1e-5), "1e-05");
        assert_eq!(fmt_g(1234567.0), "1.23457e+06");
        assert_eq!(fmt_g(12.345678), "12.3457");
        assert_eq!(fmt_g(100000.0), "100000");
        assert_eq!(fmt_g(0.0001), "0.0001");
        assert_eq!(fmt_g(999999.5), "1e+06");
    }

    #[test]
    fn repr_matches_python() {
        assert_eq!(py_repr("plain"), "'plain'");
        assert_eq!(py_repr("it's"), "\"it's\"");
        assert_eq!(py_repr("a'b\"c"), "'a\\'b\"c'");
        assert_eq!(py_repr("pog 川"), "'pog 川'");
        assert_eq!(py_repr("tab\there"), "'tab\\there'");
    }
}
