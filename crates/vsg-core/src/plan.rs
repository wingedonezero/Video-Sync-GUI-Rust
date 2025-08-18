
use crate::types::*;

pub fn positive_only(raw: &RawDelays) -> PositiveDelays {
    let min_raw = [Some(0), raw.sec_ms, raw.ter_ms].into_iter().flatten().min().unwrap_or(0);
    let global = if min_raw < 0 { -min_raw } else { 0 };
    let sec_res = raw.sec_ms.unwrap_or(0) + global;
    let ter_res = raw.ter_ms.unwrap_or(0) + global;
    PositiveDelays { global_ms: global, sec_residual_ms: sec_res, ter_residual_ms: ter_res }
}
