//! Starter maps. The template documents are recorded from 1.x into `data.rs`;
//! building one gives it fresh ids.

use crate::model::{self, Doc};
use crate::themes::{Template, TEMPLATES};
use serde_json::Value;
use std::collections::HashMap;

pub fn get(id: &str) -> Option<&'static Template> {
    TEMPLATES.iter().find(|t| t.id == id)
}

/// A new document from a template (the first template when `id` is unknown).
pub fn build(id: &str) -> Doc {
    let t = get(id).unwrap_or(&TEMPLATES[0]);
    let mut v: Value = serde_json::from_str(t.doc).unwrap_or(Value::Null);
    let mut ids = HashMap::new();
    fn renew(n: &mut Value, ids: &mut HashMap<String, String>) {
        if let Some(old) = n.get("id").and_then(Value::as_str).map(str::to_string) {
            let new = model::uid();
            ids.insert(old, new.clone());
            n["id"] = Value::String(new);
        }
        if let Some(Value::Array(k)) = n.get_mut("children") {
            k.iter_mut().for_each(|c| renew(c, ids));
        }
    }
    if let Some(r) = v.get_mut("root") {
        renew(r, &mut ids);
    }
    if let Some(Value::Array(f)) = v.get_mut("floating") {
        f.iter_mut().for_each(|c| renew(c, &mut ids));
    }
    if let Some(Value::Array(rs)) = v.get_mut("relations") {
        for r in rs {
            for k in ["from", "to"] {
                if let Some(new) = r.get(k).and_then(Value::as_str).and_then(|o| ids.get(o)).cloned() {
                    r[k] = Value::String(new);
                }
            }
            r["id"] = Value::String(model::uid());
        }
    }
    model::normalize_value(v).unwrap_or_else(|_| Doc::new(""))
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_template_builds() {
        for t in super::TEMPLATES {
            let d = super::build(t.id);
            assert!(!d.root.text.is_empty(), "{}", t.id);
            assert_ne!(d.root.id, "i0");
        }
    }
}
