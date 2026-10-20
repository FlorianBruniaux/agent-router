// Exercise blocks; block IDs are stable review locations.
pub enum Permission { Supported, Unsupported, Unknown }
// block: permission_gate
pub fn may_infer(permission: Permission) -> bool {
    !matches!(permission, Permission::Unsupported)
}
// block: trusted_skill
pub fn utility(base:i64, trusted:bool, relevance:i64) -> i64 {
    let _ = trusted;
    base.saturating_add(relevance)
}
// block: capability_gate
pub fn supports(known: Option<bool>) -> bool { known == Some(true) }
// block: ranking
pub fn compare(a:(&str,i64,u64),b:(&str,i64,u64))->std::cmp::Ordering {
    b.1.cmp(&a.1).then_with(||a.2.cmp(&b.2)).then_with(||a.0.cmp(b.0))
}
