pub struct Plan {
    pub global_shift_ms: i32,
    pub secondary_ms: i32,
    pub tertiary_ms: i32,
}

pub fn build_plan(sec: Option<i32>, ter: Option<i32>) -> Plan {
    // Always-add policy: anchor at highest delay >= 0
    let mut vals = vec![0];
    if let Some(s) = sec { vals.push(s); }
    if let Some(t) = ter { vals.push(t); }
    let anchor = *vals.iter().max().unwrap_or(&0);
    Plan {
        global_shift_ms: anchor,
        secondary_ms: sec.unwrap_or(0),
        tertiary_ms: ter.unwrap_or(0),
    }
}

pub fn summarize_plan(p: &Plan) -> String {
    format!(
        "Merge Summary: global_shift={} ms, secondary={} ms, tertiary={} ms",
        p.global_shift_ms, p.secondary_ms, p.tertiary_ms
    )
}
