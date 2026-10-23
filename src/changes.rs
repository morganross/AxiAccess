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
        .filter(|(k, v)| before.get(*k).is_some_and(|old| old != *v))
        .map(|(k, v)| json!({"nativeId":k,"node":v}))
        .collect();
    json!({"added":added,"removed":removed,"modified":modified})
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
    #[test]fn native_control_state_and_order_are_changes(){let node=|id:&str,checked:bool|serde_json::from_value::<TreeNode>(json!({"id":id,"native_id":id,"native_role":"AXCheckBox","states":{"checked":checked},"role":"checkbox","name":"same","depth":0,"offscreen":false,"focused":false,"enabled":true,"has_text":false,"text_source":null,"total_chars":0,"offscreen_chars":0,"preview":"","children":[]})).unwrap();let before=indexed(&[node("a",false),node("b",false)]).unwrap();let after=indexed(&[node("b",false),node("a",true)]).unwrap();let delta=diff(&before,&after);assert_eq!(delta["modified"].as_array().unwrap().len(),2);}
}
