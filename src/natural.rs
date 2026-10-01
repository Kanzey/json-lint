//! Natural key ordering ("a2" < "a10") and the recursive key-sorting fixer.

use std::cmp::Ordering;

use serde_json::{Map, Value};

/// Compares strings with ASCII digit runs ordered by numeric value.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.as_bytes(), b.as_bytes());
    loop {
        match (a.first(), b.first()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let (na, ra) = split_digits(a);
                let (nb, rb) = split_digits(b);
                let ord = na.len().cmp(&nb.len()).then_with(|| na.cmp(nb));
                if ord != Ordering::Equal {
                    return ord;
                }
                (a, b) = (ra, rb);
            }
            // A digit run ends the text before it, so it sorts before any other char.
            (Some(x), Some(_)) if x.is_ascii_digit() => return Ordering::Less,
            (Some(_), Some(y)) if y.is_ascii_digit() => return Ordering::Greater,
            (Some(x), Some(y)) => {
                if x != y {
                    return x.cmp(y);
                }
                (a, b) = (&a[1..], &b[1..]);
            }
        }
    }
}

/// Splits off a leading digit run, returning it without leading zeros.
fn split_digits(s: &[u8]) -> (&[u8], &[u8]) {
    let end = s.iter().position(|c| !c.is_ascii_digit()).unwrap_or(s.len());
    let zeros = s[..end].iter().take_while(|&&c| c == b'0').count();
    (&s[zeros..end], &s[end..])
}

/// Recursively sorts every object's keys in natural order (stable on ties).
pub fn sort_keys(v: &mut Value) {
    match v {
        Value::Array(items) => items.iter_mut().for_each(sort_keys),
        Value::Object(map) => {
            let mut entries: Vec<_> = std::mem::take(map).into_iter().collect();
            entries.sort_by(|(a, _), (b, _)| natural_cmp(a, b));
            *map = entries
                .into_iter()
                .map(|(k, mut v)| {
                    sort_keys(&mut v);
                    (k, v)
                })
                .collect::<Map<_, _>>();
        }
        _ => {}
    }
}

/// The `keys_are_sorted` rule.
pub fn keys_are_sorted(v: &Value) -> bool {
    match v {
        Value::Array(items) => items.iter().all(keys_are_sorted),
        Value::Object(map) => {
            let keys: Vec<_> = map.keys().collect();
            keys.windows(2).all(|w| natural_cmp(w[0], w[1]) != Ordering::Greater)
                && map.values().all(keys_are_sorted)
        }
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn natural_order() {
        assert_eq!(natural_cmp("a2", "a10"), Ordering::Less);
        assert_eq!(natural_cmp("a01", "a1"), Ordering::Equal);
        assert_eq!(natural_cmp("", "a"), Ordering::Less);
        assert_eq!(natural_cmp("10b", "9b"), Ordering::Greater);
        assert_eq!(natural_cmp("x1", "x1y"), Ordering::Less);
        assert_eq!(natural_cmp("B", "a"), Ordering::Less);
        assert_eq!(natural_cmp("20Z", "-a"), Ordering::Less);
        assert_eq!(natural_cmp("a1", "a "), Ordering::Less);
    }

    #[test]
    fn sorts_nested() {
        let mut v = json!([{"b10": {"z": 1, "y": 2}, "b9": 0, "a01": 1, "a1": 2}]);
        sort_keys(&mut v);
        assert_eq!(v.to_string(), r#"[{"a01":1,"a1":2,"b9":0,"b10":{"y":2,"z":1}}]"#);
        assert!(keys_are_sorted(&v));
    }
}
