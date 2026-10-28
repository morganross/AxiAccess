use crate::engine::TreeNode;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub fn indexed(tree: &[TreeNode]) -> Option<BTreeMap<String, Value>> {
    let mut map = BTreeMap::new();
    let mut stack: Vec<(&TreeNode, Option<String>, usize)> =
        tree.iter().enumerate().map(|(i, n)| (n, None, i)).collect();
    while let Some((node, parent, index)) = stack.pop() {
        let key = node.native_id.as_ref()?;
        if map.insert(key.clone(),json!({"parent":parent,"index":index,"role":node.role,"nativeRole":node.native_role,"states":node.states,"name":node.name,"focused":node.focused,"enabled":node.enabled,"offscreen":node.offscreen,"preview":node.preview,"totalChars":node.total_chars,"protected":node.protected})).is_some(){return None;}
        stack.extend(
            node.children
                .iter()
                .enumerate()
                .map(|(i, n)| (n, Some(key.clone()), i)),
        );
    }
    Some(map)
}
pub fn diff(before: &BTreeMap<String, Value>, after: &BTreeMap<String, Value>) -> Value {
    let added: Vec<_> = after
        .iter()
        .filter(|(k, _)| !before.contains_key(*k))
        .map(|(k, v)| json!({"nativeId":k,"node":v}))
        .collect();
    let removed: Vec<_> = before
        .keys()
        .filter(|k| !after.contains_key(*k))
        .cloned()
        .collect();
    let modified: Vec<_> = after
        .iter()
        .filter_map(|(k, v)| {
            let old = before.get(k)?;
            if old == v {
                return None;
            }
            let changed_fields: Vec<String> = v
                .as_object()
                .into_iter()
                .flatten()
                .filter_map(|(field, new_val)| {
                    (old.get(field) != Some(new_val)).then(|| field.clone())
                })
                .collect();
            Some(json!({"nativeId":k,"changedFields":changed_fields,"node":v}))
        })
        .collect();
    json!({"added":added,"removed":removed,"modified":modified})
}

/// Compute bounded inline text deltas (appended or modified slices) between two captures.
pub fn compute_text_deltas(
    before_buffers: &[(String, String, &crate::engine::TextBufferInfo)],
    after_buffers: &[(String, String, &crate::engine::TextBufferInfo)],
) -> Vec<Value> {
    const MAX_DELTA_BYTES: usize = 4_096;
    let mut deltas = Vec::new();
    for (after_ref, after_key, after_buf) in after_buffers {
        let matched = before_buffers
            .iter()
            .find(|(_, b_key, _)| b_key == after_key)
            .or_else(|| {
                before_buffers
                    .iter()
                    .find(|(_, _, b_buf)| b_buf.role == after_buf.role && b_buf.name == after_buf.name)
            });
        match matched {
            None => {
                let mut sample = after_buf.full_text.clone();
                let truncated = crate::text::truncate_utf8(&mut sample, MAX_DELTA_BYTES);
                deltas.push(json!({
                    "bufferRef": after_ref,
                    "name": after_buf.name,
                    "role": after_buf.role,
                    "changeType": "added_buffer",
                    "deltaBytes": after_buf.full_text.len(),
                    "deltaText": sample,
                    "truncated": truncated,
                }));
            }
            Some((_, _, before_buf)) => {
                if before_buf.full_text == after_buf.full_text {
                    continue;
                }
                let b_text = &before_buf.full_text;
                let a_text = &after_buf.full_text;
                if !b_text.is_empty() && a_text.starts_with(b_text.as_str()) {
                    let appended = &a_text[b_text.len()..];
                    let (slice, start_in_appended) = crate::text::tail_page(appended, MAX_DELTA_BYTES);
                    deltas.push(json!({
                        "bufferRef": after_ref,
                        "name": after_buf.name,
                        "role": after_buf.role,
                        "changeType": "appended",
                        "offsetBytes": b_text.len() + start_in_appended,
                        "deltaBytes": appended.len(),
                        "deltaText": slice,
                        "truncated": start_in_appended > 0,
                    }));
                } else {
                    // Find common UTF-8 prefix and suffix
                    let mut prefix = b_text
                        .bytes()
                        .zip(a_text.bytes())
                        .take_while(|(x, y)| x == y)
                        .count();
                    while prefix > 0 && (!b_text.is_char_boundary(prefix) || !a_text.is_char_boundary(prefix)) {
                        prefix -= 1;
                    }
                    let b_rem = &b_text[prefix..];
                    let a_rem = &a_text[prefix..];
                    let mut suffix = b_rem
                        .bytes()
                        .rev()
                        .zip(a_rem.bytes().rev())
                        .take_while(|(x, y)| x == y)
                        .count();
                    while suffix > 0
                        && (!b_rem.is_char_boundary(b_rem.len() - suffix)
                            || !a_rem.is_char_boundary(a_rem.len() - suffix))
                    {
                        suffix -= 1;
                    }
                    let inserted = &a_rem[..a_rem.len() - suffix];
                    let mut sample = inserted.to_string();
                    let truncated = crate::text::truncate_utf8(&mut sample, MAX_DELTA_BYTES);
                    deltas.push(json!({
                        "bufferRef": after_ref,
                        "name": after_buf.name,
                        "role": after_buf.role,
                        "changeType": "modified",
                        "offsetBytes": prefix,
                        "removedBytes": b_rem.len() - suffix,
                        "insertedBytes": inserted.len(),
                        "deltaText": sample,
                        "truncated": truncated,
                    }));
                }
            }
        }
    }
    deltas
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn symmetric_changes() {
        let a = BTreeMap::from([("old".into(), json!(1)), ("same".into(), json!(2))]);
        let b = BTreeMap::from([("new".into(), json!(1)), ("same".into(), json!(3))]);
        let d = diff(&a, &b);
        assert_eq!(d["added"].as_array().unwrap().len(), 1);
        assert_eq!(d["removed"][0], "old");
        assert_eq!(d["modified"][0]["nativeId"], "same");
    }
    #[test]fn native_control_state_and_order_are_changes(){let node=|id:&str,checked:bool|serde_json::from_value::<TreeNode>(json!({"id":id,"native_id":id,"native_role":"AXCheckBox","states":{"checked":checked},"role":"checkbox","name":"same","depth":0,"offscreen":false,"focused":false,"enabled":true,"has_text":false,"text_source":null,"total_chars":0,"offscreen_chars":0,"preview":"","children":[]})).unwrap();let before=indexed(&[node("a",false),node("b",false)]).unwrap();let after=indexed(&[node("b",false),node("a",true)]).unwrap();let delta=diff(&before,&after);assert_eq!(delta["modified"].as_array().unwrap().len(),2);assert!(delta["modified"][0]["changedFields"].as_array().unwrap().len() >= 1);}
    #[test]
    fn text_deltas_detect_append_and_middle_modification() {
        let mk = |text: &str| crate::engine::TextBufferInfo {
            id: "n1".into(),
            role: "document".into(),
            name: "log".into(),
            source: "TextPattern".into(),
            total_chars: text.chars().count(),
            visible_chars: 0,
            offscreen_chars: 0,
            total_lines: 1,
            visible_lines: 0,
            preview: String::new(),
            full_text: text.into(),
            visible_text: String::new(),
            truncated: false,
            protected: false,
            visibility_known: false,
        };
        let b1 = mk("line1\nline2\n");
        let b2 = mk("line1\nline2\nline3\n");
        let deltas = compute_text_deltas(
            &[("buf1".into(), "native-1".into(), &b1)],
            &[("buf2".into(), "native-1".into(), &b2)],
        );
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0]["changeType"], "appended");
        assert_eq!(deltas[0]["deltaText"], "line3\n");

        let b3 = mk("line1\nCHANGED\nline3\n");
        let deltas_mod = compute_text_deltas(
            &[("buf2".into(), "native-1".into(), &b2)],
            &[("buf3".into(), "native-1".into(), &b3)],
        );
        assert_eq!(deltas_mod.len(), 1);
        assert_eq!(deltas_mod[0]["changeType"], "modified");
        assert_eq!(deltas_mod[0]["deltaText"], "CHANGED");
    }
}
