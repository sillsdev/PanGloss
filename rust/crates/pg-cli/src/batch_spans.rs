//! `batch --morph-spans`: each `--analyses` reading's per-morph surface spans, in `generate`'s shape plus the surface text.

use pg_parse::morpher::ParseSpans;
use serde_json::{json, Value};

/// The `morphSpans` object for one reading, or `null` when no sound span answer exists for it.
pub(crate) fn render(spans: Option<&ParseSpans>) -> Value {
    let Some(parsed) = spans else {
        return Value::Null;
    };
    let mut value = crate::serve_generate::render_surface_spans(&parsed.spans);
    value["surface"] = json!(parsed.surface);
    let text = |span: &mut Value| {
        let slice = span["offsets"]
            .as_array()
            .and_then(|o| Some((o.first()?.as_u64()? as usize, o.get(1)?.as_u64()? as usize)))
            .and_then(|(start, end)| parsed.surface.get(start..end));
        span["text"] = json!(slice);
    };
    for morpheme in value["morphemes"].as_array_mut().into_iter().flatten() {
        for span in morpheme["spans"].as_array_mut().into_iter().flatten() {
            text(span);
        }
    }
    for span in value["unattributed"].as_array_mut().into_iter().flatten() {
        text(span);
    }
    value
}
